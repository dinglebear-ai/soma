use std::time::Duration;

use serde_json::Value;
use soma_fleet::HostId;
use soma_infra::{ContainerExecRequest, HostExecManyEngine, HostExecRequest};
use soma_ops::{AuthorizationEvidence, OperationContext, OperationName, OperationPlan, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::mutation_exec::{container_spec, host_many_spec, host_spec};
use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute_exec(
        &self,
        operation: &OperationName,
        parameters: &Value,
        context: &OperationContext,
        plan: &OperationPlan,
        authorization: &AuthorizationEvidence,
        cancellation: &CancellationToken,
    ) -> Result<soma_ops::OperationResult, ExecutionError> {
        let started = Timestamp::now();
        self.mutation_spec(operation)?;
        self.catalog.validate_parameters(operation, parameters)?;
        let current_plan = self.plan_exec(operation, parameters, context).await?;
        validate_exec_admission(
            operation,
            context,
            plan,
            &current_plan,
            authorization,
            started,
        )?;
        match operation.as_str() {
            "container.exec" => {
                let execution = container_spec(parameters)?;
                let host = self.resolve_host(&execution.host).await?;
                ensure_host_revision(plan, &host)?;
                let ports = self.exec_ports(&execution.host)?;
                let target =
                    crate::mutation_runtime::container_target(&host, &execution.container)?;
                let deadline = bounded_deadline(context, started, i64::from(execution.timeout_ms));
                let request = ContainerExecRequest::new(
                    context.operation_id().clone(),
                    operation.clone(),
                    execution.container,
                    execution.command,
                    execution.user,
                    execution.working_dir,
                    deadline,
                )?;
                let client = match ports.containers.exec_client(&host, cancellation).await {
                    Ok(client) => client,
                    Err(error) => {
                        return self.exec_failure_result(
                            operation,
                            context,
                            target,
                            started,
                            soma_ops::MutationSendState::NotSent,
                            error,
                            false,
                        );
                    }
                };
                match client.exec_container(&host, &request, cancellation).await {
                    Ok(receipt) => {
                        self.container_exec_result(operation, context, target, started, receipt)
                    }
                    Err(failure) => self.exec_failure_result(
                        operation,
                        context,
                        target,
                        started,
                        failure.send_state(),
                        failure.into_error(),
                        false,
                    ),
                }
            }
            "host.exec" => {
                let execution = host_spec(parameters)?;
                let host = self.resolve_host(&execution.host).await?;
                ensure_host_revision(plan, &host)?;
                let ports = self.exec_ports(&execution.host)?;
                let target = crate::mutation_exec::host_target(&host)?;
                let request = HostExecRequest::new(
                    context.operation_id().clone(),
                    operation.clone(),
                    execution.command,
                    execution.args,
                    execution.working_dir,
                    bounded_deadline(context, started, i64::from(execution.timeout_secs) * 1_000),
                )?;
                match ports.hosts.exec_host(&host, &request, cancellation).await {
                    Ok(receipt) => {
                        self.host_exec_result(operation, context, target, started, receipt)
                    }
                    Err(failure) => self.exec_failure_result(
                        operation,
                        context,
                        target,
                        started,
                        failure.send_state(),
                        failure.into_error(),
                        false,
                    ),
                }
            }
            "host.exec_many" => {
                let execution = host_many_spec(parameters)?;
                let snapshot = self.ports.hosts.snapshot().await?;
                if plan.topology_revision() != Some(snapshot.revision().as_str()) {
                    return Err(ExecutionError::PlanMismatch(
                        "fleet topology changed after execution admission".into(),
                    ));
                }
                let ports = self.exec_ports("fanout")?;
                let mut targets = Vec::with_capacity(execution.targets.len());
                for selected in execution.targets {
                    let id = HostId::new(&selected.host).map_err(|error| {
                        ExecutionError::InvalidParameter {
                            field: "targets.host".into(),
                            message: error.to_string(),
                        }
                    })?;
                    let host = snapshot
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| ExecutionError::HostNotFound(selected.host.clone()))?;
                    let request = HostExecRequest::new(
                        context.operation_id().clone(),
                        operation.clone(),
                        execution.command,
                        execution.args.clone(),
                        selected.working_dir,
                        bounded_deadline(
                            context,
                            started,
                            i64::from(execution.timeout_secs) * 1_000,
                        ),
                    )?;
                    targets.push((host, request));
                }
                let concurrency = ports.max_fanout_concurrency.clamp(1, 8).min(targets.len());
                let engine = HostExecManyEngine::new(
                    concurrency,
                    Duration::from_secs(u64::from(execution.timeout_secs)),
                )?;
                let target = plan.target().clone();
                match engine
                    .execute(ports.hosts.as_ref(), targets, cancellation.clone())
                    .await
                {
                    Ok(outcome) => {
                        self.host_exec_many_result(operation, context, target, started, outcome)
                    }
                    Err(failure) => self.exec_many_failure_result(
                        operation,
                        context,
                        target,
                        started,
                        failure.send_state(),
                        failure.into_error(),
                    ),
                }
            }
            _ => Err(ExecutionError::UnsupportedOperation(operation.clone())),
        }
    }

    fn exec_ports(&self, host: &str) -> Result<&crate::SynapseExecPorts, ExecutionError> {
        self.ports
            .exec
            .as_ref()
            .ok_or_else(|| ExecutionError::MutationPortUnavailable {
                domain: "exec",
                host: host.to_owned(),
            })
    }
}

fn validate_exec_admission(
    operation: &OperationName,
    context: &OperationContext,
    plan: &OperationPlan,
    current: &OperationPlan,
    authorization: &AuthorizationEvidence,
    now: Timestamp,
) -> Result<(), ExecutionError> {
    plan.validate_fingerprint()?;
    if plan != current {
        return Err(ExecutionError::PlanMismatch(
            "execution argv, timeout, paths, targets, or topology changed after planning".into(),
        ));
    }
    if plan.verification().is_some() {
        return Err(ExecutionError::PlanMismatch(
            "exec operations do not support a fabricated verification strategy".into(),
        ));
    }
    if context.deadline().is_some_and(|deadline| deadline <= now) {
        return Err(ExecutionError::DeadlineExceeded);
    }
    if authorization.confirmation_ref().is_none() {
        return Err(ExecutionError::ConfirmationRequired);
    }
    authorization.validate_binding(operation, plan.target(), now, Some(plan.fingerprint()))?;
    Ok(())
}

fn ensure_host_revision(
    plan: &OperationPlan,
    host: &soma_fleet::HostRecord,
) -> Result<(), ExecutionError> {
    if plan.topology_revision() != Some(host.revision().as_str()) {
        Err(ExecutionError::PlanMismatch(
            "host topology revision changed after execution admission".into(),
        ))
    } else {
        Ok(())
    }
}

fn bounded_deadline(
    context: &OperationContext,
    started: Timestamp,
    requested_millis: i64,
) -> Timestamp {
    let requested = Timestamp::from_unix_millis(
        started
            .unix_millis()
            .saturating_add(requested_millis.max(1)),
    );
    context
        .deadline()
        .map_or(requested, |deadline| deadline.min(requested))
}

#[cfg(test)]
#[path = "mutation_exec_execute_tests.rs"]
mod tests;
