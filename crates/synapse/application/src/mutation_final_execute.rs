use serde_json::Value;
use soma_infra::{ComposeDownRequest, DockerPruneRequest, DockerPruneTarget, ImageRemovalRequest};
use soma_ops::{AuthorizationEvidence, OperationContext, OperationName, OperationPlan, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::mutation_compose::{compose_target, resolve_project};
use crate::mutation_final_admission::{
    final_execution_deadline as execution_deadline, validate_final_admission,
    validate_final_changes as validate_changes,
};
use crate::mutation_final_contract::{
    compose_down_change, docker_target, image_target, prune_change, rmi_change,
};
use crate::runtime_params::{bool_or, required_str};
use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute_final(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        cancellation: &CancellationToken,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        match operation.as_str() {
            "docker.rmi" => {
                self.execute_rmi(
                    operation,
                    parameters,
                    context,
                    plan,
                    authorization,
                    cancellation,
                )
                .await
            }
            "docker.prune" => {
                self.execute_prune(
                    operation,
                    parameters,
                    context,
                    plan,
                    authorization,
                    cancellation,
                )
                .await
            }
            "compose.down" => {
                self.execute_compose_down(
                    operation,
                    parameters,
                    context,
                    plan,
                    authorization,
                    cancellation,
                )
                .await
            }
            "files.transfer" => {
                self.execute_transfer(
                    operation,
                    parameters,
                    context,
                    plan,
                    authorization,
                    cancellation,
                )
                .await
            }
            _ => Err(ExecutionError::UnsupportedOperation(operation.clone())),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_rmi(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        cancellation: &CancellationToken,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        let started = Timestamp::now();
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let image = required_str(parameters, "image")?;
        let force = bool_or(parameters, "force", false)?;
        let target = image_target(&host, image)?;
        let expected = self.plan_final(operation, parameters, context).await?;
        validate_final_admission(
            operation,
            context,
            plan,
            authorization,
            &expected,
            started,
            spec.idempotent(),
        )?;
        let ports = self.final_ports("docker-cleanup")?;
        let client = ports.cleanup.cleanup_client(&host, cancellation).await?;
        let fingerprint = self
            .docker_cleanup
            .inspect_image(client.as_ref(), &host, image, cancellation)
            .await?;
        validate_changes(plan, &[rmi_change(&host, &fingerprint, force)?])?;
        let request = ImageRemovalRequest {
            operation_id: context.operation_id().clone(),
            operation: operation.clone(),
            fingerprint,
            force,
            deadline: execution_deadline(context, started),
        };
        match self
            .docker_cleanup
            .remove_image(client.as_ref(), &host, &request, cancellation)
            .await
        {
            Ok(outcome) => {
                self.rmi_outcome_result(operation, context, target, started, spec.retry(), outcome)
            }
            Err(failure) => self.failure_result(
                operation,
                context,
                target,
                started,
                failure.send_state(),
                spec.retry(),
                failure.into_error(),
                None,
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_prune(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        cancellation: &CancellationToken,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        let started = Timestamp::now();
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let target_kind = DockerPruneTarget::parse(required_str(parameters, "prune_target")?)?;
        let force = bool_or(parameters, "force", false)?;
        let target = docker_target(&host)?;
        let expected = self.plan_final(operation, parameters, context).await?;
        validate_final_admission(
            operation,
            context,
            plan,
            authorization,
            &expected,
            started,
            spec.idempotent(),
        )?;
        let ports = self.final_ports("docker-cleanup")?;
        let client = ports.cleanup.cleanup_client(&host, cancellation).await?;
        let fingerprint = self
            .docker_cleanup
            .inspect_prune(client.as_ref(), &host, target_kind, cancellation)
            .await?;
        validate_changes(plan, &[prune_change(&host, &fingerprint, force)?])?;
        let request = DockerPruneRequest {
            operation_id: context.operation_id().clone(),
            operation: operation.clone(),
            fingerprint,
            force,
            deadline: execution_deadline(context, started),
        };
        match self
            .docker_cleanup
            .prune(client.as_ref(), &host, &request, cancellation)
            .await
        {
            Ok(outcome) => self.prune_outcome_result(
                operation,
                context,
                target,
                started,
                spec.retry(),
                outcome,
            ),
            Err(failure) => self.failure_result(
                operation,
                context,
                target,
                started,
                failure.send_state(),
                spec.retry(),
                failure.into_error(),
                None,
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_compose_down(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        cancellation: &CancellationToken,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        let started = Timestamp::now();
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let project_name = required_str(parameters, "project")?;
        let force = bool_or(parameters, "force", false)?;
        let remove_volumes = bool_or(parameters, "remove_volumes", false)?;
        let target = compose_target(&host, project_name)?;
        let expected = self.plan_final(operation, parameters, context).await?;
        validate_final_admission(
            operation,
            context,
            plan,
            authorization,
            &expected,
            started,
            spec.idempotent(),
        )?;
        let ports = self.final_ports("compose-down")?;
        let deadline = execution_deadline(context, started);
        let project = resolve_project(
            ports.compose_down.as_ref(),
            &host,
            project_name,
            deadline,
            cancellation,
        )
        .await?;
        let (fingerprint, _) = self
            .compose_down
            .inspect(
                ports.compose_down.as_ref(),
                &host,
                &project,
                deadline,
                cancellation,
            )
            .await?;
        validate_changes(
            plan,
            &[compose_down_change(
                &host,
                &fingerprint,
                force,
                remove_volumes,
            )?],
        )?;
        let request = ComposeDownRequest::new(
            context.operation_id().clone(),
            operation.clone(),
            project,
            fingerprint,
            force,
            remove_volumes,
            deadline,
        )?;
        match self
            .compose_down
            .execute(ports.compose_down.as_ref(), &host, &request, cancellation)
            .await
        {
            Ok(outcome) => self.compose_down_outcome_result(
                operation,
                context,
                target,
                started,
                spec.retry(),
                outcome,
            ),
            Err(failure) => self.failure_result(
                operation,
                context,
                target,
                started,
                failure.send_state(),
                spec.retry(),
                failure.into_error(),
                None,
            ),
        }
    }
}

#[cfg(test)]
#[path = "mutation_final_execute_tests.rs"]
mod tests;
