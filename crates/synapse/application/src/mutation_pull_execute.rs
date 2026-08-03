use serde_json::Value;
use soma_infra::{ComposePullRequest, ImagePullRequest, MutationProgressReporter};
use soma_ops::{
    AuthorizationEvidence, OperationContext, OperationName, OperationPlan, TargetRef, Timestamp,
};
use tokio_util::sync::CancellationToken;

use crate::mutation_compose::{compose_target, resolve_project};
use crate::mutation_pull::{
    configured_images, image_target, resolve_container_image, validate_pull_changes,
};
use crate::mutation_runtime::{DEFAULT_MUTATION_DEADLINE_MS, container_target};
use crate::runtime_params::{optional_str, required_str};
use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute_pull(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        match operation.as_str() {
            "docker.pull" => {
                self.execute_docker_pull(
                    operation,
                    parameters,
                    context,
                    plan,
                    authorization,
                    progress,
                    cancellation,
                )
                .await
            }
            "container.pull" => {
                self.execute_container_pull(
                    operation,
                    parameters,
                    context,
                    plan,
                    authorization,
                    progress,
                    cancellation,
                )
                .await
            }
            "compose.pull" => {
                self.execute_compose_pull(
                    operation,
                    parameters,
                    context,
                    plan,
                    authorization,
                    progress,
                    cancellation,
                )
                .await
            }
            _ => Err(ExecutionError::UnsupportedOperation(operation.clone())),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_docker_pull(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        let started_at = Timestamp::now();
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let image = required_str(parameters, "image")?;
        let target = image_target(&host, image)?;
        self.validate_admission(
            operation,
            context,
            plan,
            authorization,
            &target,
            &host,
            started_at,
            spec.idempotent(),
            "docker.images",
        )?;
        validate_pull_changes(plan, std::slice::from_ref(&target))?;
        let deadline = mutation_deadline(context, started_at);
        let client = self.artifact_client(&host, cancellation).await?;
        let request = ImagePullRequest::new(
            context.operation_id().clone(),
            operation.clone(),
            image,
            deadline,
        )?;
        match self
            .image_pull
            .execute(client.as_ref(), &host, &request, progress, cancellation)
            .await
        {
            Ok(outcome) => self.image_pull_outcome_result(
                operation,
                context,
                target,
                started_at,
                spec.retry(),
                outcome,
                None,
            ),
            Err(failure) => self.pull_failure_result(
                operation,
                context,
                target,
                started_at,
                spec.retry(),
                failure,
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_container_pull(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        let started_at = Timestamp::now();
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let container = required_str(parameters, "container_id")?;
        let target = container_target(&host, container)?;
        let client = self.artifact_client(&host, cancellation).await?;
        let image =
            resolve_container_image(client.as_ref(), &host, container, cancellation).await?;
        let image_resource = image_target(&host, &image)?;
        self.validate_admission(
            operation,
            context,
            plan,
            authorization,
            &target,
            &host,
            started_at,
            spec.idempotent(),
            "docker.images",
        )?;
        validate_pull_changes(plan, std::slice::from_ref(&image_resource))?;
        let request = ImagePullRequest::new(
            context.operation_id().clone(),
            operation.clone(),
            &image,
            mutation_deadline(context, started_at),
        )?;
        match self
            .image_pull
            .execute(client.as_ref(), &host, &request, progress, cancellation)
            .await
        {
            Ok(outcome) => self.image_pull_outcome_result(
                operation,
                context,
                target,
                started_at,
                spec.retry(),
                outcome,
                Some(container),
            ),
            Err(failure) => self.pull_failure_result(
                operation,
                context,
                target,
                started_at,
                spec.retry(),
                failure,
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_compose_pull(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        let started_at = Timestamp::now();
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let project_name = required_str(parameters, "project")?;
        let service = optional_str(parameters, "service")?;
        let target = compose_target(&host, project_name)?;
        let compose = self.compose_pull_client(&host)?;
        let deadline = mutation_deadline(context, started_at);
        let project = resolve_project(
            compose.as_ref(),
            &host,
            project_name,
            deadline,
            cancellation,
        )
        .await?;
        let config = compose
            .config(&host, &project, deadline, cancellation)
            .await?;
        let images = configured_images(&config, service)?;
        let expected = images
            .iter()
            .map(|(_, image)| image_target(&host, image))
            .collect::<Result<Vec<_>, ExecutionError>>()?;
        self.validate_admission(
            operation,
            context,
            plan,
            authorization,
            &target,
            &host,
            started_at,
            spec.idempotent(),
            "docker.images",
        )?;
        validate_pull_changes(plan, &expected)?;
        let artifacts = self.artifact_client(&host, cancellation).await?;
        let request = ComposePullRequest::new(
            context.operation_id().clone(),
            operation.clone(),
            project,
            service.map(str::to_owned),
            deadline,
        )?;
        match self
            .compose_pull
            .execute(
                compose.as_ref(),
                artifacts.as_ref(),
                &host,
                &request,
                progress,
                cancellation,
            )
            .await
        {
            Ok(outcome) => self.compose_pull_outcome_result(
                operation,
                context,
                target,
                started_at,
                spec.retry(),
                outcome,
            ),
            Err(failure) => self.pull_failure_result(
                operation,
                context,
                target,
                started_at,
                spec.retry(),
                failure,
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn pull_failure_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started_at: Timestamp,
        retry: soma_ops::RetryClass,
        failure: soma_infra::MutationFailure,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        self.failure_result(
            operation,
            context,
            target,
            started_at,
            failure.send_state(),
            retry,
            failure.into_error(),
            None,
        )
    }
}

fn mutation_deadline(context: &OperationContext, started_at: Timestamp) -> Timestamp {
    context.deadline().unwrap_or_else(|| {
        Timestamp::from_unix_millis(
            started_at
                .unix_millis()
                .saturating_add(DEFAULT_MUTATION_DEADLINE_MS),
        )
    })
}

#[cfg(test)]
#[path = "mutation_pull_execute_tests.rs"]
mod tests;
