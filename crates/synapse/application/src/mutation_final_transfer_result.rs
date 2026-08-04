use std::path::Path;

use serde_json::json;
use soma_infra::{InfraError, VerifiedFileTransferOutcome};
use soma_ops::{
    ArtifactRef, Diagnostic, DiagnosticSeverity, EvidenceRef, ExecutionMetadata, MutationSendState,
    OperationContext, OperationName, OperationResult, OperationStatus, RetryClass, TargetRef,
    Timestamp, VerificationResult, VerificationStatus,
};

use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    pub(crate) fn transfer_outcome_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started: Timestamp,
        outcome: VerifiedFileTransferOutcome,
    ) -> Result<OperationResult, ExecutionError> {
        let artifact_uri = format!("soma-artifact://{}", context.operation_id());
        let output = json!({
            "source": outcome.before.source.path.display().to_string(),
            "destination": outcome.before.destination_path.display().to_string(),
            "bytes": outcome.bytes,
            "source_digest": outcome.before.source.sha256,
            "destination_digest": outcome.destination_after.sha256,
            "verified": outcome.verified,
            "artifact": {
                "uri": artifact_uri,
                "media_type": "application/octet-stream",
                "bytes": outcome.bytes,
                "sha256": outcome.destination_after.sha256,
                "protected": true,
            },
        });
        self.catalog.validate_result(operation, &output)?;
        let execution = ExecutionMetadata::new(
            started,
            Timestamp::now(),
            outcome.send_state,
            RetryClass::Never,
        )?;
        let artifact = ArtifactRef::new(
            format!("soma-artifact://{}", context.operation_id()),
            "application/octet-stream",
            true,
        )?
        .with_sha256(outcome.destination_after.sha256.clone())?;
        let mut result = OperationResult::new(
            context.operation_id().clone(),
            operation.clone(),
            target,
            OperationStatus::Succeeded,
            execution,
        )?
        .with_output(output)?
        .with_artifact(artifact)
        .with_evidence(EvidenceRef::new(
            "artifact",
            format!("file-transfer://{}", context.operation_id()),
        )?);
        result = result.with_verification(VerificationResult::new(
            VerificationStatus::Verified,
            Timestamp::now(),
        ))?;
        result.validate()?;
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn transfer_failure_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started: Timestamp,
        send_state: MutationSendState,
        retry: RetryClass,
        error: InfraError,
        source: &Path,
        destination: &Path,
    ) -> Result<OperationResult, ExecutionError> {
        let output = json!({
            "source": source.display().to_string(),
            "destination": destination.display().to_string(),
            "bytes": 0,
            "verified": false,
        });
        self.catalog.validate_result(operation, &output)?;
        let execution = ExecutionMetadata::new(started, Timestamp::now(), send_state, retry)?;
        let (code, verification) = if send_state == MutationSendState::Unknown {
            ("mutation.uncertain", VerificationStatus::Inconclusive)
        } else {
            ("verification.failed", VerificationStatus::Failed)
        };
        let mut result = OperationResult::new(
            context.operation_id().clone(),
            operation.clone(),
            target,
            OperationStatus::Failed,
            execution,
        )?
        .with_output(output)?
        .with_diagnostic(
            Diagnostic::new(code, DiagnosticSeverity::Error, error.to_string())?
                .with_next_action(
                    "inspect both source and destination digests before planning a selective retry or restoring the prior destination",
                )?,
        );
        result =
            result.with_verification(VerificationResult::new(verification, Timestamp::now()))?;
        result.validate()?;
        Ok(result)
    }
}

#[cfg(test)]
#[path = "mutation_final_transfer_result_tests.rs"]
mod tests;
