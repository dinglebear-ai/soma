use serde_json::json;
use soma_infra::{ComposeRecreateOutcome, ContainerRecreateOutcome};
use soma_ops::{
    EvidenceRef, ExecutionMetadata, OperationContext, OperationName, OperationResult,
    OperationStatus, RetryClass, TargetRef, Timestamp, VerificationResult, VerificationStatus,
};

use crate::mutation_pull_result::verification_diagnostic;
use crate::mutation_result::mutation_output;
use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn container_recreate_outcome_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started_at: Timestamp,
        failure_retry: RetryClass,
        outcome: ContainerRecreateOutcome,
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
                "container {} was replaced by {} and verified running",
                outcome.original_container,
                outcome.new_container.as_deref().unwrap_or("unknown")
            )
        } else {
            format!(
                "container {} replacement reached {:?} without verified completion",
                outcome.original_container, outcome.stage
            )
        };
        let output = mutation_output(
            operation,
            outcome.changed,
            summary,
            Some(outcome.topology_revision.as_str()),
            json!({
                "host": outcome.host,
                "original_container": outcome.original_container,
                "new_container": outcome.new_container,
                "pulled": outcome.pulled,
                "stage": outcome.stage,
                "before": outcome.before,
                "after": outcome.after,
                "send_state": outcome.send_state,
                "verification": outcome.verification,
            }),
        );
        self.catalog.validate_result(operation, &output)?;
        let execution = ExecutionMetadata::new(
            started_at,
            completed_at,
            outcome.send_state,
            if verified {
                RetryClass::Never
            } else {
                failure_retry
            },
        )?;
        let original = target.id().to_owned();
        let replacement = outcome
            .new_container
            .clone()
            .unwrap_or_else(|| "unknown".into());
        let mut result = OperationResult::new(
            context.operation_id().clone(),
            operation.clone(),
            target,
            status,
            execution,
        )?
        .with_output(output)?
        .with_verification(VerificationResult::new(
            outcome.verification_status,
            Timestamp::now(),
        ))?
        .with_evidence(EvidenceRef::new(
            "diff",
            container_diff_uri(&outcome.host.to_string(), &original, &replacement),
        )?);
        if let Some(new_container) = &outcome.new_container {
            result = result.with_evidence(EvidenceRef::new(
                "runtime_state",
                format!("docker-container://{}/{}", outcome.host, new_container),
            )?);
        }
        if !verified {
            result = result.with_diagnostic(verification_diagnostic(
                outcome.verification_status,
                outcome.verification.summary,
                "inspect the replacement stage and recreate the captured name from the original image/configuration evidence before retrying",
            )?);
        }
        result.validate()?;
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compose_recreate_outcome_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started_at: Timestamp,
        failure_retry: RetryClass,
        outcome: ComposeRecreateOutcome,
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
                "Compose project {} was force-recreated and verified healthy",
                outcome.project
            )
        } else {
            format!(
                "Compose project {} force-recreate lacked verified completion",
                outcome.project
            )
        };
        let output = mutation_output(
            operation,
            outcome.changed,
            summary,
            Some(outcome.topology_revision.as_str()),
            json!({
                "host": outcome.host,
                "project": outcome.project,
                "before": outcome.before,
                "after": outcome.after,
                "stdout": outcome.stdout,
                "stderr": outcome.stderr,
                "output_truncated": outcome.output_truncated,
                "send_state": outcome.send_state,
                "verification": outcome.verification,
            }),
        );
        self.catalog.validate_result(operation, &output)?;
        let execution = ExecutionMetadata::new(
            started_at,
            completed_at,
            outcome.send_state,
            if verified {
                RetryClass::Never
            } else {
                failure_retry
            },
        )?;
        let uri = compose_diff_uri(&outcome.host.to_string(), &outcome.project);
        let mut result = OperationResult::new(
            context.operation_id().clone(),
            operation.clone(),
            target,
            status,
            execution,
        )?
        .with_output(output)?
        .with_verification(VerificationResult::new(
            outcome.verification_status,
            Timestamp::now(),
        ))?
        .with_evidence(EvidenceRef::new("diff", &uri)?)
        .with_evidence(EvidenceRef::new("runtime_state", uri)?);
        if !verified {
            result = result.with_diagnostic(verification_diagnostic(
                outcome.verification_status,
                outcome.verification.summary,
                "inspect Compose status and logs, restore the prior configuration if necessary, and do not retry blindly",
            )?);
        }
        result.validate()?;
        Ok(result)
    }
}

fn container_diff_uri(host: &str, original: &str, replacement: &str) -> String {
    format!("container-recreate://{host}/{original}/{replacement}")
}

fn compose_diff_uri(host: &str, project: &str) -> String {
    format!("compose-recreate://{host}/{project}")
}

#[cfg(test)]
#[path = "mutation_recreate_result_tests.rs"]
mod tests;
