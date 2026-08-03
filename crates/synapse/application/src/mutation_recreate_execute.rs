use serde_json::Value;
use soma_infra::{ComposeRecreateRequest, ContainerRecreateRequest, compose_recreate_fingerprint};
use soma_ops::{AuthorizationEvidence, OperationContext, OperationName, OperationPlan, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::mutation_compose::{compose_target, resolve_project};
use crate::mutation_recreate::{compose_recreate_change, container_recreate_change};
use crate::mutation_runtime::{DEFAULT_MUTATION_DEADLINE_MS, container_target};
use crate::runtime_params::{bool_or, required_str};
use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute_recreate(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        cancellation: &CancellationToken,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        match operation.as_str() {
            "container.recreate" => {
                self.execute_container_recreate(
                    operation,
                    parameters,
                    context,
                    plan,
                    authorization,
                    cancellation,
                )
                .await
            }
            "compose.recreate" => {
                self.execute_compose_recreate(
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
    async fn execute_container_recreate(
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
        let container = required_str(parameters, "container_id")?;
        let pull = bool_or(parameters, "pull", true)?;
        let target = container_target(&host, container)?;
        let client = self
            .recreate_ports(&host)?
            .containers
            .recreate_client(&host, cancellation)
            .await?;
        let fingerprint = client
            .recreate_fingerprint(&host, container, cancellation)
            .await?;
        self.validate_admission(
            operation,
            context,
            plan,
            authorization,
            &target,
            &host,
            started,
            spec.idempotent(),
            "container.inspect",
        )?;
        validate_changes(
            plan,
            &[container_recreate_change(&host, &fingerprint, pull)?],
        )?;
        let request = ContainerRecreateRequest::new(
            context.operation_id().clone(),
            operation.clone(),
            fingerprint,
            pull,
            deadline(context, started),
        );
        match self
            .container_recreate
            .execute(client.as_ref(), &host, &request, cancellation)
            .await
        {
            Ok(outcome) => self.container_recreate_outcome_result(
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
    async fn execute_compose_recreate(
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
        let target = compose_target(&host, project_name)?;
        let ports = self.recreate_ports(&host)?;
        let deadline = deadline(context, started);
        let project = resolve_project(
            ports.compose.as_ref(),
            &host,
            project_name,
            deadline,
            cancellation,
        )
        .await?;
        let config = ports
            .compose
            .config(&host, &project, deadline, cancellation)
            .await?;
        let status = ports
            .compose
            .status(&host, &project, None, deadline, cancellation)
            .await?;
        let fingerprint = compose_recreate_fingerprint(&config, &status)?;
        self.validate_admission(
            operation,
            context,
            plan,
            authorization,
            &target,
            &host,
            started,
            spec.idempotent(),
            "compose.status",
        )?;
        validate_changes(plan, &[compose_recreate_change(&host, &fingerprint)?])?;
        let request = ComposeRecreateRequest::new(
            context.operation_id().clone(),
            operation.clone(),
            project,
            fingerprint,
            deadline,
        );
        match self
            .compose_recreate
            .execute(ports.compose.as_ref(), &host, &request, cancellation)
            .await
        {
            Ok(outcome) => self.compose_recreate_outcome_result(
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

fn validate_changes(
    plan: &OperationPlan,
    expected: &[soma_ops::PlannedChange],
) -> Result<(), ExecutionError> {
    if plan.changes() != expected {
        return Err(ExecutionError::PlanMismatch(
            "replacement target, pull choice, configuration fingerprint, or service set changed after planning".into(),
        ));
    }
    Ok(())
}

fn deadline(context: &OperationContext, started: Timestamp) -> Timestamp {
    context.deadline().unwrap_or_else(|| {
        Timestamp::from_unix_millis(
            started
                .unix_millis()
                .saturating_add(DEFAULT_MUTATION_DEADLINE_MS),
        )
    })
}

#[cfg(test)]
#[path = "mutation_recreate_execute_tests.rs"]
mod tests;
