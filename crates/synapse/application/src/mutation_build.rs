use serde_json::Value;
use soma_fleet::HostRecord;
use soma_infra::{BuildContextFingerprint, ComposeBuildArtifact, ComposeConfig, ComposeProjectRef};
use soma_ops::{
    OperationContext, OperationName, OperationPlan, PlanStep, PlannedChange, Timestamp,
    VerificationStrategy,
};
use tokio_util::sync::CancellationToken;

use crate::mutation_compose::{compose_target, resolve_project};
use crate::mutation_pull::image_target;
use crate::mutation_runtime::DEFAULT_MUTATION_DEADLINE_MS;
use crate::runtime_params::{optional_str, required_path, required_str};
use crate::{ExecutionError, SynapseBuildPorts, SynapseMutationRuntime};

pub(crate) fn build_operation(operation: &OperationName) -> bool {
    matches!(operation.as_str(), "docker.build" | "compose.build")
}

impl SynapseMutationRuntime {
    pub(crate) async fn plan_build(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
    ) -> Result<OperationPlan, ExecutionError> {
        match operation.as_str() {
            "docker.build" => self.plan_docker_build(operation, parameters, context).await,
            "compose.build" => {
                self.plan_compose_build(operation, parameters, context)
                    .await
            }
            _ => Err(ExecutionError::UnsupportedOperation(operation.clone())),
        }
    }

    async fn plan_docker_build(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
    ) -> Result<OperationPlan, ExecutionError> {
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let path = required_path(parameters, "context")?;
        let tag = required_str(parameters, "tag")?;
        let ports = self.build_ports(&host)?;
        let deadline = planning_deadline(context);
        let fingerprint = ports
            .contexts
            .fingerprint(&host, &path, deadline, &CancellationToken::new())
            .await?;
        let target = image_target(&host, tag)?;
        let change = docker_change(&host, tag, &path, &fingerprint)?;
        let step = PlanStep::new(
            1,
            operation.clone(),
            target.clone(),
            format!("build image {tag} from {}", path.display()),
        )?;
        let verification = VerificationStrategy::new(
            OperationName::new("docker.images").expect("static operation"),
            format!("verify tag {tag} resolves to a local image identity"),
        )?;
        OperationPlan::new(context.operation_id().clone(),operation.clone(),target,spec.risk(),spec.reversibility())?
   .with_topology_revision(host.revision().to_string())?
   .with_change(change)?
   .with_prerequisite(format!("context {} remains sha256:{} ({} files, {} bytes)",path.display(),fingerprint.sha256,fingerprint.file_count,fingerprint.byte_count))?
   .with_step(step)?.with_verification(verification)?
   .with_rollback_guidance("restore the previous image tag or rebuild from the previously authorized context digest")
   .map_err(ExecutionError::from)
    }

    async fn plan_compose_build(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
    ) -> Result<OperationPlan, ExecutionError> {
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let project_name = required_str(parameters, "project")?;
        let service = optional_str(parameters, "service")?;
        let compose = self.compose_pull_client(&host)?;
        let ports = self.build_ports(&host)?;
        let deadline = planning_deadline(context);
        let project = resolve_project(
            compose.as_ref(),
            &host,
            project_name,
            deadline,
            &CancellationToken::new(),
        )
        .await?;
        let config = compose
            .config(&host, &project, deadline, &CancellationToken::new())
            .await?;
        let artifacts = compose_artifacts(
            ports,
            &host,
            &project,
            &config,
            service,
            deadline,
            &CancellationToken::new(),
        )
        .await?;
        let target = compose_target(&host, project_name)?;
        let mut plan = OperationPlan::new(
            context.operation_id().clone(),
            operation.clone(),
            target.clone(),
            spec.risk(),
            spec.reversibility(),
        )?
        .with_topology_revision(host.revision().to_string())?;
        for artifact in &artifacts {
            plan = plan.with_change(compose_change(&host, artifact)?)?;
        }
        let step = PlanStep::new(
            1,
            operation.clone(),
            target.clone(),
            format!(
                "build {} Compose image artifact(s) for project {project_name}",
                artifacts.len()
            ),
        )?;
        let verification = VerificationStrategy::new(
            OperationName::new("docker.images").expect("static operation"),
            "verify every selected Compose output tag resolves locally",
        )?;
        plan.with_prerequisite(format!("Compose config {} and every selected context digest remain unchanged",project.config_file().display()))?
   .with_step(step)?.with_verification(verification)?
   .with_rollback_guidance("restore the previous image tags or rebuild each service from its previously authorized context digest")
   .map_err(ExecutionError::from)
    }

    pub(crate) fn build_ports(
        &self,
        host: &HostRecord,
    ) -> Result<&SynapseBuildPorts, ExecutionError> {
        self.ports
            .builds
            .as_ref()
            .ok_or_else(|| ExecutionError::MutationPortUnavailable {
                domain: "build",
                host: host.id().to_string(),
            })
    }
}

pub(crate) fn docker_change(
    host: &HostRecord,
    tag: &str,
    path: &std::path::Path,
    fingerprint: &BuildContextFingerprint,
) -> Result<PlannedChange, ExecutionError> {
    Ok(PlannedChange::new(
        image_target(host, tag)?,
        "build",
        format!("build image {tag} from {}", path.display()),
    )?
    .with_digests(Some(fingerprint.sha256.clone()), None))
}
pub(crate) fn compose_change(
    host: &HostRecord,
    artifact: &ComposeBuildArtifact,
) -> Result<PlannedChange, ExecutionError> {
    Ok(PlannedChange::new(
        image_target(host, &artifact.image)?,
        "build",
        format!(
            "build service {} as {} from {}",
            artifact.service,
            artifact.image,
            artifact.context.display()
        ),
    )?
    .with_digests(Some(artifact.fingerprint.sha256.clone()), None))
}

pub(crate) async fn compose_artifacts(
    ports: &SynapseBuildPorts,
    host: &HostRecord,
    project: &ComposeProjectRef,
    config: &ComposeConfig,
    service: Option<&str>,
    deadline: Timestamp,
    cancellation: &CancellationToken,
) -> Result<Vec<ComposeBuildArtifact>, ExecutionError> {
    let mut artifacts = Vec::new();
    for (name, spec) in &config.services {
        if service.is_some_and(|selected| selected != name) {
            continue;
        }
        let Some(raw_context) = spec.build_context.as_deref() else {
            if service == Some(name.as_str()) {
                return Err(invalid("service", "selected service has no build context"));
            }
            continue;
        };
        let image = spec.image.clone().ok_or_else(|| {
            invalid(
                "service",
                "build-enabled services require an explicit image tag",
            )
        })?;
        let path = soma_infra::resolve_compose_build_context(project.config_file(), raw_context)?;
        let fingerprint = ports
            .contexts
            .fingerprint(host, &path, deadline, cancellation)
            .await?;
        artifacts.push(ComposeBuildArtifact {
            service: name.clone(),
            image,
            context: path,
            fingerprint,
        });
    }
    if artifacts.is_empty() {
        return Err(invalid(
            "service",
            "no build-enabled Compose services were selected",
        ));
    }
    Ok(artifacts)
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
fn invalid(field: &str, message: &str) -> ExecutionError {
    ExecutionError::InvalidParameter {
        field: field.into(),
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "mutation_build_tests.rs"]
mod tests;
