use serde_json::{Value, json};
use soma_infra::{ContainerExecReceipt, HostExecManyOutcome, HostExecReceipt, InfraError};
use soma_ops::{
    Diagnostic, DiagnosticSeverity, EvidenceRef, ExecutionMetadata, MutationSendState,
    OperationContext, OperationName, OperationResult, OperationStatus, RetryClass, TargetRef,
    Timestamp,
};

use crate::mutation_exec_output::{exec_output, failure_code, many_output};
use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    pub(crate) fn container_exec_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started: Timestamp,
        receipt: ContainerExecReceipt,
    ) -> Result<OperationResult, ExecutionError> {
        let output = exec_output(
            receipt.exit_code,
            receipt.stdout,
            receipt.stderr,
            false,
            receipt.truncated,
        );
        self.exec_terminal_result(
            operation,
            context,
            target,
            started,
            receipt.send_state,
            output,
            exec_evidence_uri(
                "container-exec",
                &receipt.host.to_string(),
                &receipt.container,
            ),
            receipt.exit_code == Some(0),
            "internal.failure",
        )
    }

    pub(crate) fn host_exec_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started: Timestamp,
        receipt: HostExecReceipt,
    ) -> Result<OperationResult, ExecutionError> {
        let output = exec_output(
            receipt.exit_code.map(i64::from),
            receipt.stdout,
            receipt.stderr,
            false,
            receipt.truncated,
        );
        self.exec_terminal_result(
            operation,
            context,
            target,
            started,
            receipt.send_state,
            output,
            exec_evidence_uri(
                "host-exec",
                &receipt.host.to_string(),
                receipt.command.as_str(),
            ),
            receipt.exit_code == Some(0),
            "command.failed",
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn exec_terminal_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started: Timestamp,
        send_state: MutationSendState,
        output: Value,
        evidence: String,
        succeeded: bool,
        failure_code: &'static str,
    ) -> Result<OperationResult, ExecutionError> {
        self.catalog.validate_result(operation, &output)?;
        let completed = Timestamp::now();
        let status = if succeeded {
            OperationStatus::Succeeded
        } else {
            OperationStatus::Failed
        };
        let execution = ExecutionMetadata::new(started, completed, send_state, RetryClass::Never)?;
        let mut result = OperationResult::new(
            context.operation_id().clone(),
            operation.clone(),
            target,
            status,
            execution,
        )?
        .with_output(output)?
        .with_evidence(EvidenceRef::new("logs", evidence)?);
        if !succeeded {
            result = result.with_diagnostic(
                Diagnostic::new(
                    failure_code,
                    DiagnosticSeverity::Error,
                    "command completed without a zero exit status",
                )?
                .with_next_action(
                    "inspect captured stdout and stderr, correct the command or target state, and create a new plan before retrying",
                )?,
            );
        }
        result.validate()?;
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn exec_failure_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started: Timestamp,
        send_state: MutationSendState,
        error: InfraError,
        timed_out: bool,
    ) -> Result<OperationResult, ExecutionError> {
        let output = exec_output(None, String::new(), String::new(), timed_out, false);
        self.catalog.validate_result(operation, &output)?;
        let code = failure_code(&error, send_state, timed_out);
        let completed = Timestamp::now();
        let execution = ExecutionMetadata::new(started, completed, send_state, RetryClass::Never)?;
        let result = OperationResult::new(
            context.operation_id().clone(),
            operation.clone(),
            target,
            if matches!(error, InfraError::Fleet(soma_fleet::FleetError::Cancelled)) {
                OperationStatus::Cancelled
            } else {
                OperationStatus::Failed
            },
            execution,
        )?
        .with_output(output)?
        .with_diagnostic(
            Diagnostic::new(code, DiagnosticSeverity::Error, error.to_string())?.with_next_action(
                "do not retry blindly; inspect target state and create a fresh execution plan",
            )?,
        );
        result.validate()?;
        Ok(result)
    }

    pub(crate) fn host_exec_many_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started: Timestamp,
        outcome: HostExecManyOutcome,
    ) -> Result<OperationResult, ExecutionError> {
        let output = many_output(&outcome);
        self.catalog.validate_result(operation, &output)?;
        let succeeded = outcome.all_succeeded();
        let execution = ExecutionMetadata::new(
            started,
            Timestamp::now(),
            outcome.send_state,
            RetryClass::Never,
        )?;
        let mut result = OperationResult::new(
            context.operation_id().clone(),
            operation.clone(),
            target,
            if succeeded {
                OperationStatus::Succeeded
            } else {
                OperationStatus::Failed
            },
            execution,
        )?
        .with_output(output)?
        .with_evidence(EvidenceRef::new(
            "logs",
            format!("host-exec-many://{}", context.operation_id()),
        )?);
        if !succeeded {
            result = result.with_diagnostic(
                Diagnostic::new(
                    if outcome.send_state == MutationSendState::Unknown {
                        "mutation.uncertain"
                    } else {
                        "command.failed"
                    },
                    DiagnosticSeverity::Error,
                    format!(
                        "fanout completed with {} successes, {} failures, {} cancellations, and {} timeouts",
                        outcome.succeeded, outcome.failed, outcome.cancelled, outcome.timed_out
                    ),
                )?
                .with_next_action(
                    "inspect each target result and create a new plan containing only unresolved hosts",
                )?,
            );
        }
        result.validate()?;
        Ok(result)
    }

    pub(crate) fn exec_many_failure_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started: Timestamp,
        send_state: MutationSendState,
        error: InfraError,
    ) -> Result<OperationResult, ExecutionError> {
        let output = json!({
            "results": [],
            "success_count": 0,
            "failure_count": 0,
            "cancelled_count": 0,
        });
        self.catalog.validate_result(operation, &output)?;
        let execution =
            ExecutionMetadata::new(started, Timestamp::now(), send_state, RetryClass::Never)?;
        let result = OperationResult::new(
            context.operation_id().clone(),
            operation.clone(),
            target,
            OperationStatus::Failed,
            execution,
        )?
        .with_output(output)?
        .with_diagnostic(
            Diagnostic::new(
                failure_code(&error, send_state, false),
                DiagnosticSeverity::Error,
                error.to_string(),
            )?
            .with_next_action(
                "no complete target report exists; inspect fleet state and create a fresh fanout plan",
            )?,
        );
        result.validate()?;
        Ok(result)
    }
}

fn exec_evidence_uri(kind: &str, host: &str, target: &str) -> String {
    format!("{kind}://{host}/{target}")
}

#[cfg(test)]
#[path = "mutation_exec_result_tests.rs"]
mod tests;
