use serde_json::Value;
use soma_fleet::HostRecord;
use soma_infra::{
    ComposeRecreateFingerprint, ContainerRecreateFingerprint, compose_recreate_fingerprint,
};
use soma_ops::{
    OperationContext, OperationName, OperationPlan, PlanStep, PlannedChange, Timestamp,
    VerificationStrategy,
};
use tokio_util::sync::CancellationToken;

use crate::mutation_compose::{compose_target, resolve_project};
use crate::mutation_runtime::{DEFAULT_MUTATION_DEADLINE_MS, container_target};
use crate::runtime_params::{bool_or, required_str};
use crate::{ExecutionError, SynapseMutationRuntime, SynapseRecreatePorts};

impl SynapseMutationRuntime {
    pub(crate) async fn plan_recreate(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
    ) -> Result<OperationPlan, ExecutionError> {
        match operation.as_str() {
            "container.recreate" => {
                self.plan_container_recreate(operation, parameters, context)
                    .await
            }
            "compose.recreate" => {
                self.plan_compose_recreate(operation, parameters, context)
                    .await
            }
            _ => Err(ExecutionError::UnsupportedOperation(operation.clone())),
        }
    }

    async fn plan_container_recreate(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
    ) -> Result<OperationPlan, ExecutionError> {
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let container = required_str(parameters, "container_id")?;
        let pull = bool_or(parameters, "pull", true)?;
        let target = container_target(&host, container)?;
        let cancellation = CancellationToken::new();
        let client = self
            .recreate_ports(&host)?
            .containers
            .recreate_client(&host, &cancellation)
            .await?;
        let fingerprint = client
            .recreate_fingerprint(&host, container, &cancellation)
            .await?;
        let change = container_recreate_change(&host, &fingerprint, pull)?;
        let summary = if pull {
            format!(
                "pull {} and replace container {container}",
                fingerprint.image
            )
        } else {
            format!("replace container {container} without pulling its image")
        };
        let step = PlanStep::new(1, operation.clone(), target.clone(), summary.clone())?;
        let verification = VerificationStrategy::new(
            OperationName::new("container.inspect").expect("static operation name"),
            format!(
                "inspect the replacement until container {} is running under name {}",
                container, fingerprint.name
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
        .with_prerequisite("the replacement-relevant container configuration remains unchanged")?
        .with_prerequisite("persistent volumes and external dependencies are independently recoverable")?
        .with_step(step)?
        .with_verification(verification)?
        .with_rollback_guidance(
            "if replacement fails after removal, recreate the original name from the captured image and configuration evidence",
        )
        .map_err(ExecutionError::from)
    }

    async fn plan_compose_recreate(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
    ) -> Result<OperationPlan, ExecutionError> {
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let project_name = required_str(parameters, "project")?;
        let target = compose_target(&host, project_name)?;
        let ports = self.recreate_ports(&host)?;
        let deadline = planning_deadline(context);
        let cancellation = CancellationToken::new();
        let project = resolve_project(
            ports.compose.as_ref(),
            &host,
            project_name,
            deadline,
            &cancellation,
        )
        .await?;
        let config = ports
            .compose
            .config(&host, &project, deadline, &cancellation)
            .await?;
        let status = ports
            .compose
            .status(&host, &project, None, deadline, &cancellation)
            .await?;
        let fingerprint = compose_recreate_fingerprint(&config, &status)?;
        let change = compose_recreate_change(&host, &fingerprint)?;
        let summary = format!("force-recreate Compose project {project_name}");
        let step = PlanStep::new(1, operation.clone(), target.clone(), summary)?;
        let verification = VerificationStrategy::new(
            OperationName::new("compose.status").expect("static operation name"),
            "verify the complete configured service set is running and healthy",
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
        .with_prerequisite("the Compose configuration and current service set remain unchanged")?
        .with_prerequisite("persistent volumes and external services are independently recoverable")?
        .with_step(step)?
        .with_verification(verification)?
        .with_rollback_guidance(
            "restore the prior Compose configuration and run compose up only after inspecting replacement logs",
        )
        .map_err(ExecutionError::from)
    }

    pub(crate) fn recreate_ports(
        &self,
        host: &HostRecord,
    ) -> Result<&SynapseRecreatePorts, ExecutionError> {
        self.ports
            .recreate
            .as_ref()
            .ok_or_else(|| ExecutionError::MutationPortUnavailable {
                domain: "recreate",
                host: host.id().to_string(),
            })
    }
}

pub(crate) fn recreate_operation(operation: &OperationName) -> bool {
    matches!(
        operation.as_str(),
        "container.recreate" | "compose.recreate"
    )
}

pub(crate) fn container_recreate_change(
    host: &HostRecord,
    fingerprint: &ContainerRecreateFingerprint,
    pull: bool,
) -> Result<PlannedChange, ExecutionError> {
    let action = if pull { "recreate_pull" } else { "recreate" };
    Ok(PlannedChange::new(
        container_target(host, &fingerprint.container)?,
        action,
        format!(
            "replace container {} as {} from image {}",
            fingerprint.container, fingerprint.name, fingerprint.image
        ),
    )?
    .with_digests(Some(fingerprint.sha256.clone()), None))
}

pub(crate) fn compose_recreate_change(
    host: &HostRecord,
    fingerprint: &ComposeRecreateFingerprint,
) -> Result<PlannedChange, ExecutionError> {
    Ok(PlannedChange::new(
        compose_target(host, &fingerprint.project)?,
        "force_recreate",
        format!(
            "replace {} configured services in Compose project {}",
            fingerprint.services.len(),
            fingerprint.project
        ),
    )?
    .with_digests(Some(fingerprint.sha256.clone()), None))
}

fn planning_deadline(context: &OperationContext) -> Timestamp {
    context.deadline().unwrap_or_else(|| {
        Timestamp::from_unix_millis(
            Timestamp::now()
                .unix_millis()
                .saturating_add(DEFAULT_MUTATION_DEADLINE_MS),
        )
    })
}

#[cfg(test)]
#[path = "mutation_recreate_tests.rs"]
mod tests;
