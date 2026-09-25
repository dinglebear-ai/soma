use serde_json::Value;
use soma_infra::{ComposeDownRequest, DockerPruneTarget};
use soma_ops::{OperationContext, OperationName, OperationPlan, PlanStep, VerificationStrategy};
use tokio_util::sync::CancellationToken;

use crate::mutation_compose::{compose_target, resolve_project};
use crate::mutation_final_contract::{
    compose_down_change, docker_target, image_target, planning_deadline, prune_change, rmi_change,
    transfer_change, transfer_target,
};
use crate::runtime_params::{bool_or, required_path, required_str};
use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    pub(crate) async fn plan_final(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
    ) -> Result<OperationPlan, ExecutionError> {
        match operation.as_str() {
            "docker.rmi" => self.plan_rmi(operation, parameters, context).await,
            "docker.prune" => self.plan_prune(operation, parameters, context).await,
            "compose.down" => self.plan_compose_down(operation, parameters, context).await,
            "files.transfer" => self.plan_transfer(operation, parameters, context).await,
            _ => Err(ExecutionError::UnsupportedOperation(operation.clone())),
        }
    }

    async fn plan_rmi(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
    ) -> Result<OperationPlan, ExecutionError> {
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let image = required_str(parameters, "image")?;
        let force = bool_or(parameters, "force", false)?;
        let target = image_target(&host, image)?;
        let cancellation = CancellationToken::new();
        let client = self
            .final_ports("docker-cleanup")?
            .cleanup
            .cleanup_client(&host, &cancellation)
            .await?;
        let fingerprint = self
            .docker_cleanup
            .inspect_image(client.as_ref(), &host, image, &cancellation)
            .await?;
        let change = rmi_change(&host, &fingerprint, force)?;
        let step = PlanStep::new(
            1,
            operation.clone(),
            target.clone(),
            format!("remove Docker image {}", fingerprint.identity.id),
        )?;
        let verification = VerificationStrategy::new(
            OperationName::new("docker.images").expect("static operation name"),
            "verify the requested reference and resolved image ID are absent",
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
        .with_prerequisite("the exact local image identity remains unchanged")?
        .with_prerequisite("dependent containers and tags have been reviewed")?
        .with_step(step)?
        .with_verification(verification)?
        .with_rollback_guidance(
            "restore the removed image from its recorded repository digest or rebuild it from a verified source context",
        )
        .map_err(ExecutionError::from)
    }

    async fn plan_prune(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
    ) -> Result<OperationPlan, ExecutionError> {
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let prune_target = DockerPruneTarget::parse(required_str(parameters, "prune_target")?)?;
        let force = bool_or(parameters, "force", false)?;
        let target = docker_target(&host)?;
        let cancellation = CancellationToken::new();
        let client = self
            .final_ports("docker-cleanup")?
            .cleanup
            .cleanup_client(&host, &cancellation)
            .await?;
        let fingerprint = self
            .docker_cleanup
            .inspect_prune(client.as_ref(), &host, prune_target, &cancellation)
            .await?;
        let change = prune_change(&host, &fingerprint, force)?;
        let step = PlanStep::new(
            1,
            operation.clone(),
            target.clone(),
            format!("prune Docker {} resources", prune_target.as_str()),
        )?;
        let verification = VerificationStrategy::new(
            OperationName::new("docker.df").expect("static operation name"),
            "verify reported deleted identities are absent and reclaimed cache bytes are reflected",
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
        .with_prerequisite("the exact prune candidate inventory remains unchanged")?
        .with_step(step)?
        .with_verification(verification)?
        .with_rollback_guidance(
            "pruned resources are irreversible; restore images, containers, volumes, networks, or build cache only from independent backups and source definitions",
        )
        .map_err(ExecutionError::from)
    }

    async fn plan_compose_down(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
    ) -> Result<OperationPlan, ExecutionError> {
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let project_name = required_str(parameters, "project")?;
        let force = bool_or(parameters, "force", false)?;
        let remove_volumes = bool_or(parameters, "remove_volumes", false)?;
        let target = compose_target(&host, project_name)?;
        let ports = self.final_ports("compose-down")?;
        let deadline = planning_deadline(context);
        let cancellation = CancellationToken::new();
        let project = resolve_project(
            ports.compose_down.as_ref(),
            &host,
            project_name,
            deadline,
            &cancellation,
        )
        .await?;
        let (fingerprint, _) = self
            .compose_down
            .inspect(
                ports.compose_down.as_ref(),
                &host,
                &project,
                deadline,
                &cancellation,
            )
            .await?;
        ComposeDownRequest::new(
            context.operation_id().clone(),
            operation.clone(),
            project,
            fingerprint.clone(),
            force,
            remove_volumes,
            deadline,
        )?;
        let change = compose_down_change(&host, &fingerprint, force, remove_volumes)?;
        let step = PlanStep::new(
            1,
            operation.clone(),
            target.clone(),
            format!("tear down Compose project {project_name}"),
        )?;
        let verification = VerificationStrategy::new(
            OperationName::new("compose.status").expect("static operation name"),
            "verify the project reports no remaining services",
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
        .with_prerequisite("the Compose configuration and complete service set remain unchanged")?
        .with_prerequisite("persistent data is recoverable before volume deletion")?
        .with_step(step)?
        .with_verification(verification)?
        .with_rollback_guidance(
            "run a separately planned compose.up from the recorded configuration; deleted volumes require backup restoration",
        )
        .map_err(ExecutionError::from)
    }

    async fn plan_transfer(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
    ) -> Result<OperationPlan, ExecutionError> {
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let source = self
            .resolve_host(required_str(parameters, "source_host")?)
            .await?;
        let destination = self
            .resolve_host(required_str(parameters, "dest_host")?)
            .await?;
        let source_path = required_path(parameters, "source_path")?;
        let destination_path = required_path(parameters, "dest_path")?;
        let cancellation = CancellationToken::new();
        let fingerprint = self
            .file_transfer
            .inspect(
                self.final_ports("file-transfer")?.transfer.as_ref(),
                &source,
                &source_path,
                &destination,
                &destination_path,
                &cancellation,
            )
            .await?;
        let target = transfer_target(&source, &source_path, &destination, &destination_path)?;
        let change = transfer_change(&target, &fingerprint)?;
        let step = PlanStep::new(
            1,
            operation.clone(),
            target.clone(),
            format!(
                "copy {} bytes from {}:{} to {}:{}",
                fingerprint.source.bytes,
                source.id(),
                source_path.display(),
                destination.id(),
                destination_path.display()
            ),
        )?;
        let verification = VerificationStrategy::new(
            OperationName::new("files.compare").expect("static operation name"),
            "verify destination bytes and SHA-256 match the source",
        )?;
        OperationPlan::new(
            context.operation_id().clone(),
            operation.clone(),
            target,
            spec.risk(),
            spec.reversibility(),
        )?
        .with_topology_revision(destination.revision().to_string())?
        .with_change(change)?
        .with_prerequisite("the source and destination pre-state remain unchanged")?
        .with_step(step)?
        .with_verification(verification)?
        .with_rollback_guidance(
            "restore the destination from the recorded pre-transfer digest or remove it when no prior destination existed",
        )
        .map_err(ExecutionError::from)
    }
}

#[cfg(test)]
#[path = "mutation_final_tests.rs"]
mod tests;
