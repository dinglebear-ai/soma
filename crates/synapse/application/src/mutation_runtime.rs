use std::sync::Arc;

use serde_json::Value;
use soma_fleet::{HostId, HostRecord, HostRepository};
use soma_infra::{
    BuildContextInspector, ComposeBuildEngine, ComposeBuildMutator, ComposeMutationClient,
    ComposeMutationEngine, ComposePullClient, ComposePullEngine, ComposeRecreateClient,
    ComposeRecreateEngine, ContainerLifecycleAction, ContainerLifecycleEngine,
    ContainerLifecycleRequest, ContainerRecreateClientProvider, ContainerRecreateEngine,
    DockerArtifactClientProvider, DockerMutationClientProvider, ImageBuildEngine,
    ImageBuildMutator, ImagePullEngine,
};
use soma_ops::{
    AccessClass, AuthorizationEvidence, OperationContext, OperationName, OperationPlan, PlanStep,
    PlannedChange, TargetKind, TargetRef, Timestamp, VerificationStrategy,
};
use tokio_util::sync::CancellationToken;

use crate::runtime_params::required_str;
use crate::{ExecutionError, SynapseCatalog};

pub(crate) const DEFAULT_MUTATION_DEADLINE_MS: i64 = 30_000;

/// Product-owned privileged build ports.
pub struct SynapseBuildPorts {
    /// Descriptor-confined build-context inspector.
    pub contexts: Arc<dyn BuildContextInspector>,
    /// Docker image build driver.
    pub image: Arc<dyn ImageBuildMutator>,
    /// Compose build driver.
    pub compose: Arc<dyn ComposeBuildMutator>,
}

/// Product-owned replacement ports.
pub struct SynapseRecreatePorts {
    /// Host-bound container replacement client provider.
    pub containers: Arc<dyn ContainerRecreateClientProvider>,
    /// Compose force-recreate client.
    pub compose: Arc<dyn ComposeRecreateClient>,
}

/// Product-owned ports used by canonical Synapse mutations.
pub struct SynapseMutationPorts {
    /// Fleet topology source.
    pub hosts: Arc<dyn HostRepository>,
    /// Host-bound Docker mutation client provider.
    pub docker: Arc<dyn DockerMutationClientProvider>,
    /// Optional Compose lifecycle mutation client.
    pub compose: Option<Arc<dyn ComposeMutationClient>>,
    /// Optional Docker artifact mutation client provider.
    pub artifacts: Option<Arc<dyn DockerArtifactClientProvider>>,
    /// Optional Compose artifact mutation client.
    pub compose_pull: Option<Arc<dyn ComposePullClient>>,
    /// Optional privileged build ports.
    pub builds: Option<SynapseBuildPorts>,
    /// Optional destructive replacement ports.
    pub recreate: Option<SynapseRecreatePorts>,
}

/// Canonical Synapse mutation planner and executor.
pub struct SynapseMutationRuntime {
    pub(crate) catalog: &'static SynapseCatalog,
    pub(crate) ports: SynapseMutationPorts,
    lifecycle: ContainerLifecycleEngine,
    pub(crate) compose: ComposeMutationEngine,
    pub(crate) image_pull: ImagePullEngine,
    pub(crate) compose_pull: ComposePullEngine,
    pub(crate) image_build: ImageBuildEngine,
    pub(crate) compose_build: ComposeBuildEngine,
    pub(crate) container_recreate: ContainerRecreateEngine,
    pub(crate) compose_recreate: ComposeRecreateEngine,
}

impl SynapseMutationRuntime {
    /// Creates the mutation runtime from product-owned ports.
    #[must_use]
    pub fn new(ports: SynapseMutationPorts) -> Self {
        Self {
            catalog: SynapseCatalog::embedded(),
            ports,
            lifecycle: ContainerLifecycleEngine::default(),
            compose: ComposeMutationEngine::default(),
            image_pull: ImagePullEngine,
            compose_pull: ComposePullEngine,
            image_build: ImageBuildEngine,
            compose_build: ComposeBuildEngine,
            container_recreate: ContainerRecreateEngine,
            compose_recreate: ComposeRecreateEngine,
        }
    }

    /// Creates a runtime with an explicit container verification engine.
    #[must_use]
    pub fn with_lifecycle_engine(
        ports: SynapseMutationPorts,
        lifecycle: ContainerLifecycleEngine,
    ) -> Self {
        Self::with_engines(ports, lifecycle, ComposeMutationEngine::default())
    }

    /// Creates a runtime with explicit container and Compose verification engines.
    #[must_use]
    pub fn with_engines(
        ports: SynapseMutationPorts,
        lifecycle: ContainerLifecycleEngine,
        compose: ComposeMutationEngine,
    ) -> Self {
        Self {
            catalog: SynapseCatalog::embedded(),
            ports,
            lifecycle,
            compose,
            image_pull: ImagePullEngine,
            compose_pull: ComposePullEngine,
            image_build: ImageBuildEngine,
            compose_build: ComposeBuildEngine,
            container_recreate: ContainerRecreateEngine,
            compose_recreate: ComposeRecreateEngine,
        }
    }

    pub(crate) async fn plan_container(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
    ) -> Result<OperationPlan, ExecutionError> {
        let action = lifecycle_action(operation)?;
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let container = required_str(parameters, "container_id")?;
        let target = container_target(&host, container)?;
        let summary = format!(
            "{} container {container} on host {}",
            action.action_label(),
            host.id()
        );
        let change = PlannedChange::new(target.clone(), action.action_label(), summary.clone())?;
        let step = PlanStep::new(1, operation.clone(), target.clone(), summary)?;
        let verification = VerificationStrategy::new(
            OperationName::new("container.inspect").expect("static operation name"),
            format!(
                "inspect container {container} until the {} post-state is observed",
                action.action_label()
            ),
        )?;
        OperationPlan::new(
            context.operation_id().clone(),
            operation.clone(),
            target,
            spec.risk(),
            spec.reversibility(),
        )?
        .with_topology_revision(host.revision().to_string())?
        .with_change(change)?
        .with_prerequisite("the target Docker daemon is reachable")?
        .with_step(step)?
        .with_verification(verification)?
        .with_rollback_guidance(rollback_guidance(action))
        .map_err(ExecutionError::from)
    }

    pub(crate) async fn execute_container(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        cancellation: &CancellationToken,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        let started_at = Timestamp::now();
        let action = lifecycle_action(operation)?;
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let container = required_str(parameters, "container_id")?;
        let target = container_target(&host, container)?;
        self.validate_admission(
            operation,
            context,
            plan,
            authorization,
            &target,
            &host,
            started_at,
            spec.idempotent(),
            "container.inspect",
        )?;
        let deadline = context.deadline().unwrap_or_else(|| {
            Timestamp::from_unix_millis(
                started_at
                    .unix_millis()
                    .saturating_add(DEFAULT_MUTATION_DEADLINE_MS),
            )
        });
        let request = ContainerLifecycleRequest::new(container, action, deadline)?;
        let client = match self.ports.docker.mutation_client(&host, cancellation).await {
            Ok(client) => client,
            Err(error) => {
                return self.failure_result(
                    operation,
                    context,
                    target,
                    started_at,
                    soma_ops::MutationSendState::NotSent,
                    spec.retry(),
                    error,
                    None,
                );
            }
        };
        match self
            .lifecycle
            .execute(client.as_ref(), &host, &request, cancellation)
            .await
        {
            Ok(outcome) => self.outcome_result(
                operation,
                context,
                target,
                started_at,
                spec.retry(),
                outcome,
            ),
            Err(failure) => self.failure_result(
                operation,
                context,
                target,
                started_at,
                failure.send_state(),
                spec.retry(),
                failure.into_error(),
                None,
            ),
        }
    }

    pub(crate) fn mutation_spec(
        &self,
        operation: &OperationName,
    ) -> Result<&soma_ops::OperationSpec, ExecutionError> {
        let spec = self
            .catalog
            .operation(operation)
            .ok_or_else(|| crate::CompatibilityError::UnknownOperation(operation.clone()))?;
        if spec.access() != AccessClass::Mutation
            || (lifecycle_action(operation).is_err()
                && crate::mutation_compose::compose_action(operation).is_err()
                && !crate::mutation_pull::pull_operation(operation)
                && !crate::mutation_build::build_operation(operation)
                && !crate::mutation_recreate::recreate_operation(operation))
        {
            return Err(ExecutionError::UnsupportedOperation(operation.clone()));
        }
        Ok(spec)
    }

    pub(crate) async fn resolve_host(&self, name: &str) -> Result<HostRecord, ExecutionError> {
        let id = HostId::new(name).map_err(|error| ExecutionError::InvalidParameter {
            field: "host".into(),
            message: error.to_string(),
        })?;
        self.ports
            .hosts
            .snapshot()
            .await?
            .get(&id)
            .cloned()
            .ok_or_else(|| ExecutionError::HostNotFound(name.to_owned()))
    }
}

pub(crate) fn lifecycle_action(
    operation: &OperationName,
) -> Result<ContainerLifecycleAction, ExecutionError> {
    match operation.as_str() {
        "container.start" => Ok(ContainerLifecycleAction::Start),
        "container.stop" => Ok(ContainerLifecycleAction::Stop),
        "container.restart" => Ok(ContainerLifecycleAction::Restart),
        "container.pause" => Ok(ContainerLifecycleAction::Pause),
        "container.resume" => Ok(ContainerLifecycleAction::Resume),
        _ => Err(ExecutionError::UnsupportedOperation(operation.clone())),
    }
}

pub(crate) fn container_target(
    host: &HostRecord,
    container: &str,
) -> Result<TargetRef, ExecutionError> {
    TargetRef::new(TargetKind::Container, container)?
        .with_host(host.id().to_string())?
        .with_revision(host.revision().to_string())
        .map_err(ExecutionError::from)
}

fn rollback_guidance(action: ContainerLifecycleAction) -> &'static str {
    match action {
        ContainerLifecycleAction::Start => {
            "stop the container to restore the previous stopped state"
        }
        ContainerLifecycleAction::Stop => "start the container to restore service availability",
        ContainerLifecycleAction::Restart => {
            "inspect container logs and restart again only after correcting the underlying fault"
        }
        ContainerLifecycleAction::Pause => "resume the container to restore process scheduling",
        ContainerLifecycleAction::Resume => {
            "pause the container to restore the previous paused state"
        }
    }
}

#[cfg(test)]
#[path = "mutation_runtime_tests.rs"]
mod tests;
