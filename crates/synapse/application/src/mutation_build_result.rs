use serde_json::json;
use soma_infra::{BuildContextFingerprint, ComposeBuildOutcome, ImageBuildOutcome};
use soma_ops::{
    EvidenceRef, ExecutionMetadata, OperationContext, OperationName, OperationResult,
    OperationStatus, RetryClass, TargetRef, Timestamp, VerificationResult, VerificationStatus,
};

use crate::mutation_pull_result::{add_image_evidence, verification_diagnostic};
use crate::mutation_result::mutation_output;
use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn image_build_outcome_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started: Timestamp,
        failure_retry: RetryClass,
        outcome: ImageBuildOutcome,
    ) -> Result<OperationResult, ExecutionError> {
        let verified = outcome.verification_status == VerificationStatus::Verified;
        let status = if verified {
            OperationStatus::Succeeded
        } else {
            OperationStatus::Failed
        };
        let summary = if verified {
            format!(
                "image {} was built and verified from context sha256:{}",
                outcome.tag, outcome.context.sha256
            )
        } else {
            format!(
                "image {} build completed without verified output identity",
                outcome.tag
            )
        };
        let output = mutation_output(
            operation,
            outcome.changed,
            summary,
            Some(outcome.topology_revision.as_str()),
            json!({
             "host":outcome.host,"tag":outcome.tag,"context":outcome.context,"before":outcome.before,"after":outcome.after,"send_state":outcome.send_state,
             "stdout":outcome.stdout,"stderr":outcome.stderr,"output_truncated":outcome.output_truncated,"progress_delivery_errors":outcome.progress_delivery_errors,"verification":outcome.verification,
            }),
        );
        self.catalog.validate_result(operation, &output)?;
        let retry = if status == OperationStatus::Succeeded {
            RetryClass::Never
        } else {
            failure_retry
        };
        let mut result = OperationResult::new(
            context.operation_id().clone(),
            operation.clone(),
            target,
            status,
            ExecutionMetadata::new(started, Timestamp::now(), outcome.send_state, retry)?,
        )?
        .with_output(output)?
        .with_verification(VerificationResult::new(
            outcome.verification_status,
            Timestamp::now(),
        ))?;
        result = add_context_evidence(result, &outcome.context)?;
        if let Some(after) = &outcome.after {
            result = add_image_evidence(result, &outcome.host.to_string(), after)?;
        }
        if status != OperationStatus::Succeeded {
            result=result.with_diagnostic(verification_diagnostic(outcome.verification_status,outcome.verification.summary,"inspect the build logs, context digest, and local image store before creating a new plan")?);
        }
        result.validate()?;
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compose_build_outcome_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started: Timestamp,
        failure_retry: RetryClass,
        outcome: ComposeBuildOutcome,
    ) -> Result<OperationResult, ExecutionError> {
        let verified = outcome.verification_status == VerificationStatus::Verified;
        let status = if verified {
            OperationStatus::Succeeded
        } else {
            OperationStatus::Failed
        };
        let summary = if verified {
            format!(
                "Compose project {} build completed and all output images were verified",
                outcome.project
            )
        } else {
            format!(
                "Compose project {} build completed without full output verification",
                outcome.project
            )
        };
        let output = mutation_output(
            operation,
            outcome.changed,
            summary,
            Some(outcome.topology_revision.as_str()),
            json!({
             "host":outcome.host,"project":outcome.project,"service":outcome.service,"images":outcome.images,"send_state":outcome.send_state,
             "stdout":outcome.stdout,"stderr":outcome.stderr,"output_truncated":outcome.output_truncated,"progress_delivery_errors":outcome.progress_delivery_errors,"verification":outcome.verification,
            }),
        );
        self.catalog.validate_result(operation, &output)?;
        let retry = if status == OperationStatus::Succeeded {
            RetryClass::Never
        } else {
            failure_retry
        };
        let mut result = OperationResult::new(
            context.operation_id().clone(),
            operation.clone(),
            target,
            status,
            ExecutionMetadata::new(started, Timestamp::now(), outcome.send_state, retry)?,
        )?
        .with_output(output)?
        .with_verification(VerificationResult::new(
            outcome.verification_status,
            Timestamp::now(),
        ))?;
        for image in &outcome.images {
            result = add_context_evidence(result, &image.context)?;
            if let Some(after) = &image.after {
                result = add_image_evidence(result, &outcome.host.to_string(), after)?;
            }
        }
        if status != OperationStatus::Succeeded {
            result=result.with_diagnostic(verification_diagnostic(outcome.verification_status,outcome.verification.summary,"inspect Compose build logs, configured image tags, and context digests before creating a new plan")?);
        }
        result.validate()?;
        Ok(result)
    }
}

fn add_context_evidence(
    result: OperationResult,
    context: &BuildContextFingerprint,
) -> Result<OperationResult, ExecutionError> {
    let uri = format!("build-context://{}/sha256:{}", context.host, context.sha256);
    Ok(result.with_evidence(EvidenceRef::new("source_context", uri)?))
}

#[cfg(test)]
#[path = "mutation_build_result_tests.rs"]
mod tests;
