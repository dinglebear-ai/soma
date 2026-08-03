use std::sync::Arc;

use serde_json::Value;
use soma_fleet::HostRecord;
use soma_infra::{ComposeConfig, ComposePullClient, DockerArtifactClient, InfraError};
use soma_ops::{
    OperationContext, OperationName, OperationPlan, PlanStep, PlannedChange, TargetKind, TargetRef,
    Timestamp, VerificationStrategy,
};
use tokio_util::sync::CancellationToken;

use crate::mutation_compose::{compose_target, resolve_project};
use crate::mutation_runtime::{DEFAULT_MUTATION_DEADLINE_MS, container_target};
use crate::runtime_params::{optional_str, required_str};
use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    pub(crate) async fn plan_pull(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
    ) -> Result<OperationPlan, ExecutionError> {
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let deadline = planning_deadline();
        let cancellation = CancellationToken::new();
        let (target, changes, prerequisite) = match operation.as_str() {
            "docker.pull" => {
                let image = required_str(parameters, "image")?;
                let image_target = image_target(&host, image)?;
                (
                    image_target.clone(),
                    vec![planned_image_change(
                        image_target,
                        format!("pull image {image} on host {}", host.id()),
                    )?],
                    "the target Docker daemon remains reachable".to_owned(),
                )
            }
            "container.pull" => {
                let container = required_str(parameters, "container_id")?;
                let client = self.artifact_client(&host, &cancellation).await?;
                let image =
                    resolve_container_image(client.as_ref(), &host, container, &cancellation)
                        .await?;
                (
                    container_target(&host, container)?,
                    vec![planned_image_change(
                        image_target(&host, &image)?,
                        format!("pull image {image} used by container {container}"),
                    )?],
                    format!("container {container} continues to reference image {image}"),
                )
            }
            "compose.pull" => {
                let project_name = required_str(parameters, "project")?;
                let service = optional_str(parameters, "service")?;
                let client = self.compose_pull_client(&host)?;
                let project = resolve_project(
                    client.as_ref(),
                    &host,
                    project_name,
                    deadline,
                    &cancellation,
                )
                .await?;
                let config = client
                    .config(&host, &project, deadline, &cancellation)
                    .await?;
                let images = configured_images(&config, service)?;
                let changes = images
                    .iter()
                    .map(|(service, image)| {
                        planned_image_change(
                            image_target(&host, image)?,
                            format!("pull image {image} for Compose service {service}"),
                        )
                    })
                    .collect::<Result<Vec<_>, ExecutionError>>()?;
                (
                    compose_target(&host, project_name)?,
                    changes,
                    format!(
                        "the Compose project configuration {} remains discoverable",
                        project.config_file().display()
                    ),
                )
            }
            _ => return Err(ExecutionError::UnsupportedOperation(operation.clone())),
        };
        let summary = format!("pull authorized image artifacts for {}", target.id());
        let step = PlanStep::new(1, operation.clone(), target.clone(), summary)?;
        let verification = VerificationStrategy::new(
            OperationName::new("docker.images").expect("static operation name"),
            "verify every requested image reference resolves to a local content identity",
        )?;
        let mut plan = OperationPlan::new(
            context.operation_id().clone(),
            operation.clone(),
            target,
            spec.risk(),
            spec.reversibility(),
        )?
        .with_topology_revision(host.revision().to_string())?
        .with_prerequisite(prerequisite)?
        .with_step(step)?
        .with_verification(verification)?
        .with_rollback_guidance(
            "the pull does not replace running resources; retain or redeploy the previous image digest when rollback is required",
        )?;
        for change in changes {
            plan = plan.with_change(change)?;
        }
        Ok(plan)
    }

    pub(crate) async fn artifact_client(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> Result<Arc<dyn DockerArtifactClient>, ExecutionError> {
        let provider = self.ports.artifacts.as_ref().ok_or_else(|| {
            ExecutionError::MutationPortUnavailable {
                domain: "docker-artifact",
                host: host.id().to_string(),
            }
        })?;
        provider
            .artifact_client(host, cancellation)
            .await
            .map_err(ExecutionError::from)
    }

    pub(crate) fn compose_pull_client(
        &self,
        host: &HostRecord,
    ) -> Result<Arc<dyn ComposePullClient>, ExecutionError> {
        self.ports
            .compose_pull
            .clone()
            .ok_or_else(|| ExecutionError::MutationPortUnavailable {
                domain: "compose-pull",
                host: host.id().to_string(),
            })
    }
}

pub(crate) fn pull_operation(operation: &OperationName) -> bool {
    matches!(
        operation.as_str(),
        "docker.pull" | "container.pull" | "compose.pull"
    )
}

pub(crate) fn image_target(host: &HostRecord, image: &str) -> Result<TargetRef, ExecutionError> {
    TargetRef::new(TargetKind::Image, image)?
        .with_host(host.id().to_string())?
        .with_revision(host.revision().to_string())
        .map_err(ExecutionError::from)
}

pub(crate) async fn resolve_container_image(
    client: &dyn DockerArtifactClient,
    host: &HostRecord,
    container: &str,
    cancellation: &CancellationToken,
) -> Result<String, ExecutionError> {
    let containers = client
        .list_containers(
            host,
            &soma_infra::ContainerListOptions::default(),
            cancellation,
        )
        .await?;
    let row = containers
        .iter()
        .find(|row| {
            row.id.as_deref() == Some(container)
                || row
                    .names
                    .iter()
                    .any(|name| name.trim_start_matches('/') == container)
        })
        .ok_or_else(|| {
            ExecutionError::Infra(InfraError::InvalidRequest {
                domain: "container-pull",
                message: format!("container {container} was not found"),
            })
        })?;
    row.image
        .clone()
        .filter(|image| !image.is_empty())
        .ok_or_else(|| {
            ExecutionError::Infra(InfraError::InvalidRequest {
                domain: "container-pull",
                message: format!("container {container} has no configured image reference"),
            })
        })
}

pub(crate) fn configured_images(
    config: &ComposeConfig,
    selected: Option<&str>,
) -> Result<Vec<(String, String)>, ExecutionError> {
    if let Some(selected) = selected {
        let service = config.services.get(selected).ok_or_else(|| {
            ExecutionError::Infra(InfraError::InvalidRequest {
                domain: "compose-pull",
                message: format!("Compose service {selected} was not found"),
            })
        })?;
        let image = service
            .image
            .clone()
            .filter(|image| !image.is_empty())
            .ok_or_else(|| {
                ExecutionError::Infra(InfraError::InvalidRequest {
                    domain: "compose-pull",
                    message: format!("Compose service {selected} has no image reference"),
                })
            })?;
        return Ok(vec![(selected.to_owned(), image)]);
    }
    let images = config
        .services
        .iter()
        .filter_map(|(name, service)| service.image.clone().map(|image| (name.clone(), image)))
        .filter(|(_, image)| !image.is_empty())
        .collect::<Vec<_>>();
    if images.is_empty() {
        Err(ExecutionError::Infra(InfraError::InvalidRequest {
            domain: "compose-pull",
            message: "Compose project has no pullable image references".into(),
        }))
    } else {
        Ok(images)
    }
}

pub(crate) fn validate_pull_changes(
    plan: &OperationPlan,
    expected: &[TargetRef],
) -> Result<(), ExecutionError> {
    if plan
        .changes()
        .iter()
        .any(|change| change.action() != "pull")
    {
        return Err(ExecutionError::PlanMismatch(
            "pull plan contains a non-pull resource change".into(),
        ));
    }
    let mut actual = plan
        .changes()
        .iter()
        .map(|change| change.resource().clone())
        .collect::<Vec<_>>();
    let mut expected = expected.to_vec();
    actual.sort();
    expected.sort();
    if actual != expected {
        return Err(ExecutionError::PlanMismatch(
            "authorized image artifact set changed after planning".into(),
        ));
    }
    Ok(())
}

fn planned_image_change(
    resource: TargetRef,
    summary: String,
) -> Result<PlannedChange, ExecutionError> {
    PlannedChange::new(resource, "pull", summary).map_err(ExecutionError::from)
}

fn planning_deadline() -> Timestamp {
    Timestamp::from_unix_millis(
        Timestamp::now()
            .unix_millis()
            .saturating_add(DEFAULT_MUTATION_DEADLINE_MS),
    )
}

#[cfg(test)]
#[path = "mutation_pull_tests.rs"]
mod tests;
