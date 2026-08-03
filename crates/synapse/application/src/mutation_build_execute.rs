use std::path::PathBuf;

use serde_json::Value;
use soma_infra::{
    ComposeBuildRequest, ComposeBuildServices, ImageBuildRequest, ImageBuildServices,
    MutationProgressReporter,
};
use soma_ops::{AuthorizationEvidence, OperationContext, OperationName, OperationPlan, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::mutation_build::{compose_artifacts, compose_change, docker_change};
use crate::mutation_compose::{compose_target, resolve_project};
use crate::mutation_pull::image_target;
use crate::mutation_runtime::DEFAULT_MUTATION_DEADLINE_MS;
use crate::runtime_params::{bool_or, optional_str, required_path, required_str};
use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute_build(
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
            "docker.build" => {
                self.execute_docker_build(
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
            "compose.build" => {
                self.execute_compose_build(
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
    async fn execute_docker_build(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        let started = Timestamp::now();
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let path = required_path(parameters, "context")?;
        let tag = required_str(parameters, "tag")?;
        let dockerfile = optional_str(parameters, "dockerfile")?.map(PathBuf::from);
        let no_cache = bool_or(parameters, "no_cache", false)?;
        let target = image_target(&host, tag)?;
        let deadline = deadline(context, started);
        let ports = self.build_ports(&host)?;
        let current = ports
            .contexts
            .fingerprint(&host, &path, deadline, cancellation)
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
            "docker.images",
        )?;
        validate_changes(plan, &[docker_change(&host, tag, &path, &current)?])?;
        let images = self.artifact_client(&host, cancellation).await?;
        let request = ImageBuildRequest::new(
            context.operation_id().clone(),
            operation.clone(),
            path,
            dockerfile,
            tag,
            no_cache,
            current,
            deadline,
        )?;
        match self
            .image_build
            .execute(
                ImageBuildServices {
                    contexts: ports.contexts.as_ref(),
                    mutator: ports.image.as_ref(),
                    images: images.as_ref(),
                },
                &host,
                &request,
                progress,
                cancellation,
            )
            .await
        {
            Ok(outcome) => self.image_build_outcome_result(
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
    async fn execute_compose_build(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        let started = Timestamp::now();
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let project_name = required_str(parameters, "project")?;
        let service = optional_str(parameters, "service")?;
        let target = compose_target(&host, project_name)?;
        let deadline = deadline(context, started);
        let ports = self.build_ports(&host)?;
        let compose = self.compose_pull_client(&host)?;
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
        let artifacts = compose_artifacts(
            ports,
            &host,
            &project,
            &config,
            service,
            deadline,
            cancellation,
        )
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
            "docker.images",
        )?;
        let expected = artifacts
            .iter()
            .map(|artifact| compose_change(&host, artifact))
            .collect::<Result<Vec<_>, _>>()?;
        validate_changes(plan, &expected)?;
        let images = self.artifact_client(&host, cancellation).await?;
        let request = ComposeBuildRequest::new(
            context.operation_id().clone(),
            operation.clone(),
            project,
            service.map(str::to_owned),
            artifacts,
            deadline,
        )?;
        match self
            .compose_build
            .execute(
                ComposeBuildServices {
                    contexts: ports.contexts.as_ref(),
                    mutator: ports.compose.as_ref(),
                    images: images.as_ref(),
                },
                &host,
                &request,
                progress,
                cancellation,
            )
            .await
        {
            Ok(outcome) => self.compose_build_outcome_result(
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
            "build artifact set or context fingerprint changed after planning".into(),
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
#[path = "mutation_build_execute_tests.rs"]
mod tests;
