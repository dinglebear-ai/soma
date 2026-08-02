use serde_json::Value;
use soma_fleet::HostRecord;
use soma_infra::{ComposeMutationAction, ComposeMutationRequest, ComposeProjectRef};
use soma_ops::{
    AuthorizationEvidence, OperationContext, OperationName, OperationPlan, PlanStep, PlannedChange,
    TargetKind, TargetRef, Timestamp, VerificationStrategy,
};
use tokio_util::sync::CancellationToken;

use crate::mutation_runtime::DEFAULT_MUTATION_DEADLINE_MS;
use crate::runtime_params::required_str;
use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    pub(crate) async fn plan_compose(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
    ) -> Result<OperationPlan, ExecutionError> {
        let action = compose_action(operation)?;
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let project_name = required_str(parameters, "project")?;
        let client = self.compose_client(&host)?;
        let project = resolve_project(
            client.as_ref(),
            &host,
            project_name,
            Timestamp::from_unix_millis(
                Timestamp::now()
                    .unix_millis()
                    .saturating_add(DEFAULT_MUTATION_DEADLINE_MS),
            ),
            &CancellationToken::new(),
        )
        .await?;
        let target = compose_target(&host, project_name)?;
        let summary = format!(
            "{} Compose project {project_name} on host {}",
            action.action_label(),
            host.id()
        );
        let change = PlannedChange::new(target.clone(), action.action_label(), summary.clone())?;
        let step = PlanStep::new(1, operation.clone(), target.clone(), summary)?;
        let verification = VerificationStrategy::new(
            OperationName::new("compose.status").expect("static operation name"),
            format!(
                "inspect Compose project {project_name} until all reported services are running"
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
        .with_prerequisite(format!(
            "the Compose project configuration {} remains discoverable",
            project.config_file().display()
        ))?
        .with_step(step)?
        .with_verification(verification)?
        .with_rollback_guidance(compose_rollback_guidance(action))
        .map_err(ExecutionError::from)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute_compose(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        cancellation: &CancellationToken,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        let started_at = Timestamp::now();
        let action = compose_action(operation)?;
        let spec = self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let host = self.resolve_host(required_str(parameters, "host")?).await?;
        let project_name = required_str(parameters, "project")?;
        let target = compose_target(&host, project_name)?;
        self.validate_admission(
            operation,
            context,
            plan,
            authorization,
            &target,
            &host,
            started_at,
            spec.idempotent(),
            "compose.status",
        )?;
        let deadline = context.deadline().unwrap_or_else(|| {
            Timestamp::from_unix_millis(
                started_at
                    .unix_millis()
                    .saturating_add(DEFAULT_MUTATION_DEADLINE_MS),
            )
        });
        let client = self.compose_client(&host)?;
        let project =
            match resolve_project(client.as_ref(), &host, project_name, deadline, cancellation)
                .await
            {
                Ok(project) => project,
                Err(ExecutionError::Infra(error)) => {
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
                Err(error) => return Err(error),
            };
        let request = ComposeMutationRequest::new(project, action, deadline);
        match self
            .compose
            .execute(client.as_ref(), &host, &request, cancellation)
            .await
        {
            Ok(outcome) => self.compose_outcome_result(
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

    fn compose_client(
        &self,
        host: &HostRecord,
    ) -> Result<std::sync::Arc<dyn soma_infra::ComposeMutationClient>, ExecutionError> {
        self.ports
            .compose
            .clone()
            .ok_or_else(|| ExecutionError::MutationPortUnavailable {
                domain: "compose",
                host: host.id().to_string(),
            })
    }
}

pub(crate) fn compose_action(
    operation: &OperationName,
) -> Result<ComposeMutationAction, ExecutionError> {
    match operation.as_str() {
        "compose.up" => Ok(ComposeMutationAction::Up),
        "compose.restart" => Ok(ComposeMutationAction::Restart),
        _ => Err(ExecutionError::UnsupportedOperation(operation.clone())),
    }
}

fn compose_target(host: &HostRecord, project: &str) -> Result<TargetRef, ExecutionError> {
    TargetRef::new(TargetKind::ComposeProject, project)?
        .with_host(host.id().to_string())?
        .with_revision(host.revision().to_string())
        .map_err(ExecutionError::from)
}

async fn resolve_project(
    client: &dyn soma_infra::ComposeMutationClient,
    host: &HostRecord,
    project_name: &str,
    deadline: Timestamp,
    cancellation: &CancellationToken,
) -> Result<ComposeProjectRef, ExecutionError> {
    let project = client
        .list_projects(host, deadline, cancellation)
        .await?
        .into_iter()
        .find(|project| project.name == project_name)
        .ok_or_else(|| ExecutionError::ProjectNotFound {
            host: host.id().to_string(),
            project: project_name.to_owned(),
        })?;
    let config =
        project
            .config_files
            .first()
            .cloned()
            .ok_or_else(|| ExecutionError::ProjectNotFound {
                host: host.id().to_string(),
                project: project_name.to_owned(),
            })?;
    ComposeProjectRef::new(project_name, config).map_err(ExecutionError::from)
}

fn compose_rollback_guidance(action: ComposeMutationAction) -> &'static str {
    match action {
        ComposeMutationAction::Up => {
            "inspect project status and use a separately planned compose.down only after reviewing volume policy"
        }
        ComposeMutationAction::Restart => {
            "inspect service logs and restart again only after correcting the underlying fault"
        }
    }
}

#[cfg(test)]
#[path = "mutation_compose_tests.rs"]
mod tests;
