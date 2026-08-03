use serde_json::json;
use soma_infra::{ComposePullOutcome, ImageIdentity, ImagePullOutcome};
use soma_ops::{
    ArtifactRef, Diagnostic, DiagnosticSeverity, EvidenceRef, ExecutionMetadata, OperationContext,
    OperationName, OperationResult, OperationStatus, RetryClass, TargetRef, Timestamp,
    VerificationResult, VerificationStatus,
};

use crate::mutation_result::mutation_output;
use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn image_pull_outcome_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started_at: Timestamp,
        failure_retry: RetryClass,
        outcome: ImagePullOutcome,
        container: Option<&str>,
    ) -> Result<OperationResult, ExecutionError> {
        let completed_at = Timestamp::now();
        let verified = outcome.verification_status == VerificationStatus::Verified;
        let status = if verified {
            OperationStatus::Succeeded
        } else {
            OperationStatus::Failed
        };
        let summary = if verified && outcome.changed {
            format!(
                "image {} was pulled and changed content identity",
                outcome.image
            )
        } else if verified {
            format!(
                "image {} was pulled and retained its content identity",
                outcome.image
            )
        } else {
            format!(
                "image {} pull completed without verified local identity",
                outcome.image
            )
        };
        let output = mutation_output(
            operation,
            outcome.changed,
            summary,
            Some(outcome.topology_revision.as_str()),
            json!({
                "host": outcome.host,
                "container": container,
                "image": outcome.image,
                "before": outcome.before,
                "after": outcome.after,
                "send_state": outcome.send_state,
                "events": outcome.total_events,
                "progress": outcome.progress,
                "progress_truncated": outcome.progress_truncated,
                "progress_delivery_errors": outcome.progress_delivery_errors,
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
        .with_output(output)?
        .with_verification(VerificationResult::new(
            outcome.verification_status,
            Timestamp::now(),
        ))?;
        if let Some(after) = &outcome.after {
            result = add_image_evidence(result, &outcome.host.to_string(), after)?;
        }
        if status != OperationStatus::Succeeded {
            result = result.with_diagnostic(verification_diagnostic(
                outcome.verification_status,
                outcome.verification.summary,
                "inspect the local Docker image store before retrying the pull",
            )?);
        }
        result.validate()?;
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compose_pull_outcome_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started_at: Timestamp,
        failure_retry: RetryClass,
        outcome: ComposePullOutcome,
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
                "Compose project {} image pull completed and all artifacts were verified",
                outcome.project
            )
        } else {
            format!(
                "Compose project {} image pull completed without full artifact verification",
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
                "service": outcome.service,
                "images": outcome.images,
                "send_state": outcome.send_state,
                "progress_delivery_errors": outcome.progress_delivery_errors,
                "output_truncated": outcome.output_truncated,
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
        .with_output(output)?
        .with_verification(VerificationResult::new(
            outcome.verification_status,
            Timestamp::now(),
        ))?;
        for image in &outcome.images {
            if let Some(after) = &image.after {
                result = add_image_evidence(result, &outcome.host.to_string(), after)?;
            }
        }
        if status != OperationStatus::Succeeded {
            result = result.with_diagnostic(verification_diagnostic(
                outcome.verification_status,
                outcome.verification.summary,
                "inspect the Compose configuration and local image store before retrying",
            )?);
        }
        result.validate()?;
        Ok(result)
    }
}

pub(crate) fn add_image_evidence(
    result: OperationResult,
    host: &str,
    image: &ImageIdentity,
) -> Result<OperationResult, ExecutionError> {
    let uri = format!("docker-image://{host}/{}", image.id);
    let mut artifact = ArtifactRef::new(&uri, "application/vnd.oci.image", false)?;
    if let Some(digest) = image.id.strip_prefix("sha256:")
        && digest.len() == 64
        && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        artifact = artifact.with_sha256(digest.to_ascii_lowercase())?;
    }
    Ok(result
        .with_artifact(artifact)
        .with_evidence(EvidenceRef::new("runtime_state", uri)?))
}

pub(crate) fn verification_diagnostic(
    status: VerificationStatus,
    message: String,
    next_action: &str,
) -> Result<Diagnostic, ExecutionError> {
    let code = if status == VerificationStatus::Inconclusive {
        "verification.inconclusive"
    } else {
        "verification.failed"
    };
    Diagnostic::new(code, DiagnosticSeverity::Error, message)?
        .with_next_action(next_action)
        .map_err(ExecutionError::from)
}

#[cfg(test)]
#[path = "mutation_pull_result_tests.rs"]
mod tests;
