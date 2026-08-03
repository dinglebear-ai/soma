use serde_json::{Value, json};
use soma_infra::{ComposeMutationOutcome, ContainerLifecycleOutcome, InfraError};
use soma_ops::{
    Diagnostic, DiagnosticSeverity, ExecutionMetadata, MutationSendState, OperationContext,
    OperationName, OperationResult, OperationStatus, RetryClass, TargetRef, Timestamp,
    VerificationResult, VerificationStatus,
};

use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn outcome_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started_at: Timestamp,
        failure_retry: RetryClass,
        outcome: ContainerLifecycleOutcome,
    ) -> Result<OperationResult, ExecutionError> {
        let completed_at = Timestamp::now();
        let verified = outcome.verification_status == VerificationStatus::Verified;
        let status = if verified {
            OperationStatus::Succeeded
        } else {
            OperationStatus::Failed
        };
        let summary = if verified && !outcome.changed {
            format!(
                "container {} already satisfied the requested {} state",
                outcome.container,
                outcome.action.action_label()
            )
        } else if verified {
            format!(
                "container {} {} completed and was verified",
                outcome.container,
                outcome.action.action_label()
            )
        } else {
            format!(
                "container {} {} was sent but verification did not succeed",
                outcome.container,
                outcome.action.action_label()
            )
        };
        let output = mutation_output(
            operation,
            outcome.changed,
            summary,
            Some(outcome.topology_revision.as_str()),
            json!({
                "host": outcome.host,
                "container": outcome.container,
                "before": outcome.before,
                "after": outcome.after,
                "send_state": outcome.send_state,
                "verification": outcome.verification,
            }),
        );
        self.catalog.validate_result(operation, &output)?;
        let retry = if status == OperationStatus::Succeeded {
            RetryClass::Never
        } else {
            failure_retry
        };
        let execution =
            ExecutionMetadata::new(started_at, completed_at, outcome.send_state, retry)?;
        let mut result = OperationResult::new(
            context.operation_id().clone(),
            operation.clone(),
            target,
            status,
            execution,
        )?
        .with_output(output)?;
        let verification = VerificationResult::new(outcome.verification_status, Timestamp::now());
        result = result.with_verification(verification)?;
        if status != OperationStatus::Succeeded {
            let code = match outcome.verification_status {
                VerificationStatus::Inconclusive => "verification.inconclusive",
                _ => "verification.failed",
            };
            result = result.with_diagnostic(
                Diagnostic::new(
                    code,
                    DiagnosticSeverity::Error,
                    outcome.verification.summary,
                )?
                .with_next_action(
                    "inspect the container state and logs before retrying or applying rollback guidance",
                )?,
            );
        }
        result.validate()?;
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compose_outcome_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started_at: Timestamp,
        failure_retry: RetryClass,
        outcome: ComposeMutationOutcome,
    ) -> Result<OperationResult, ExecutionError> {
        let completed_at = Timestamp::now();
        let verified = outcome.verification_status == VerificationStatus::Verified;
        let status = if verified {
            OperationStatus::Succeeded
        } else {
            OperationStatus::Failed
        };
        let summary = if verified {
            format!(
                "Compose project {} {} completed and was verified",
                outcome.project,
                outcome.action.action_label()
            )
        } else {
            format!(
                "Compose project {} {} was sent but verification did not succeed",
                outcome.project,
                outcome.action.action_label()
            )
        };
        let output = mutation_output(
            operation,
            true,
            summary,
            Some(outcome.topology_revision.as_str()),
            json!({
                "host": outcome.host,
                "project": outcome.project,
                "before": outcome.before,
                "after": outcome.after,
                "send_state": outcome.send_state,
                "verification": outcome.verification,
            }),
        );
        self.catalog.validate_result(operation, &output)?;
        let retry = if status == OperationStatus::Succeeded {
            RetryClass::Never
        } else {
            failure_retry
        };
        let execution =
            ExecutionMetadata::new(started_at, completed_at, outcome.send_state, retry)?;
        let mut result = OperationResult::new(
            context.operation_id().clone(),
            operation.clone(),
            target,
            status,
            execution,
        )?
        .with_output(output)?;
        result = result.with_verification(VerificationResult::new(
            outcome.verification_status,
            Timestamp::now(),
        ))?;
        if status != OperationStatus::Succeeded {
            let code = match outcome.verification_status {
                VerificationStatus::Inconclusive => "verification.inconclusive",
                _ => "verification.failed",
            };
            result = result.with_diagnostic(
                Diagnostic::new(
                    code,
                    DiagnosticSeverity::Error,
                    outcome.verification.summary,
                )?
                .with_next_action(
                    "inspect Compose status and logs before retrying or applying rollback guidance",
                )?,
            );
        }
        result.validate()?;
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn failure_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started_at: Timestamp,
        send_state: MutationSendState,
        declared_retry: RetryClass,
        error: InfraError,
        verification_status: Option<VerificationStatus>,
    ) -> Result<OperationResult, ExecutionError> {
        let completed_at = Timestamp::now();
        let status = if matches!(error, InfraError::Fleet(soma_fleet::FleetError::Cancelled)) {
            OperationStatus::Cancelled
        } else {
            OperationStatus::Failed
        };
        let retry = retry_after_failure(send_state, declared_retry);
        let diagnostic_code = diagnostic_code(&error, send_state);
        let summary = if send_state == MutationSendState::Unknown {
            format!(
                "{} failed with uncertain backend send state",
                operation.as_str()
            )
        } else {
            format!("{} failed before verified completion", operation.as_str())
        };
        let output = mutation_output(
            operation,
            false,
            summary,
            target.revision(),
            json!({
                "host": target.host(),
                "target": target.id(),
                "send_state": send_state,
                "error": error.to_string(),
            }),
        );
        self.catalog.validate_result(operation, &output)?;
        let execution = ExecutionMetadata::new(started_at, completed_at, send_state, retry)?;
        let mut result = OperationResult::new(
            context.operation_id().clone(),
            operation.clone(),
            target,
            status,
            execution,
        )?
        .with_output(output)?
        .with_diagnostic(
            Diagnostic::new(
                diagnostic_code,
                DiagnosticSeverity::Error,
                error.to_string(),
            )?
            .with_next_action(next_action(send_state))?,
        );
        let verification_status = verification_status.unwrap_or_else(|| {
            if send_state == MutationSendState::Unknown {
                VerificationStatus::Inconclusive
            } else {
                VerificationStatus::NotRequested
            }
        });
        result = result.with_verification(VerificationResult::new(
            verification_status,
            Timestamp::now(),
        ))?;
        result.validate()?;
        Ok(result)
    }
}

pub(crate) fn mutation_output(
    operation: &OperationName,
    changed: bool,
    summary: String,
    revision: Option<&str>,
    details: Value,
) -> Value {
    let mut output = json!({
        "changed": changed,
        "action": operation.as_str(),
        "summary": summary,
        "details": details,
    });
    if let Some(revision) = revision {
        output
            .as_object_mut()
            .expect("mutation output is an object")
            .insert("target_revision".into(), Value::String(revision.into()));
    }
    output
}

fn retry_after_failure(send_state: MutationSendState, declared: RetryClass) -> RetryClass {
    match send_state {
        MutationSendState::NotSent | MutationSendState::Unknown | MutationSendState::Sent => {
            declared
        }
        MutationSendState::NotApplicable => RetryClass::Never,
        _ => RetryClass::Never,
    }
}

fn diagnostic_code(error: &InfraError, send_state: MutationSendState) -> &'static str {
    if send_state == MutationSendState::Unknown {
        return "mutation.uncertain";
    }
    match error {
        InfraError::Fleet(soma_fleet::FleetError::Cancelled) => "operation.cancelled",
        InfraError::Fleet(soma_fleet::FleetError::DeadlineExceeded) => "operation.timeout",
        InfraError::UnsupportedTarget { .. } => "capability.unsupported",
        InfraError::Docker(_) => "docker.unavailable",
        InfraError::InvalidRequest { .. } => "request.invalid",
        _ => "internal.failure",
    }
}

fn next_action(send_state: MutationSendState) -> &'static str {
    if send_state == MutationSendState::Unknown {
        "inspect the current target state before deciding whether a retry is safe"
    } else {
        "correct the reported failure and submit a newly planned and authorized mutation"
    }
}

#[cfg(test)]
#[path = "mutation_result_tests.rs"]
mod tests;
