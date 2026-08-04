use serde_json::{Value, json};
use soma_infra::{ComposeDownOutcome, DockerPruneOutcome, ImageRemovalOutcome};
use soma_ops::{
    EvidenceRef, ExecutionMetadata, OperationContext, OperationName, OperationResult,
    OperationStatus, RetryClass, TargetRef, Timestamp, VerificationResult, VerificationStatus,
};

use crate::mutation_result::mutation_output;
use crate::{ExecutionError, SynapseMutationRuntime};

impl SynapseMutationRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn rmi_outcome_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started: Timestamp,
        _failure_retry: RetryClass,
        outcome: ImageRemovalOutcome,
    ) -> Result<OperationResult, ExecutionError> {
        let output = mutation_output(
            operation,
            outcome.removed,
            format!(
                "removed Docker image {} resolved from {}",
                outcome.before.identity.id, outcome.before.reference
            ),
            target.revision(),
            json!({
                "reference": outcome.before.reference,
                "image_id": outcome.before.identity.id,
                "repo_tags": outcome.before.identity.repo_tags,
                "repo_digests": outcome.before.identity.repo_digests,
                "deleted": outcome.receipt.deleted,
                "untagged": outcome.receipt.untagged,
                "verified_absent": outcome.removed,
            }),
        );
        self.verified_cleanup_result(
            operation,
            context,
            target,
            started,
            outcome.receipt.send_state,
            output,
            "diff",
            format!("docker-image-removal://{}", context.operation_id()),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prune_outcome_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started: Timestamp,
        _failure_retry: RetryClass,
        outcome: DockerPruneOutcome,
    ) -> Result<OperationResult, ExecutionError> {
        let scopes = outcome
            .receipt
            .scopes
            .iter()
            .map(|scope| {
                json!({
                    "target": scope.target,
                    "deleted_count": scope.deleted.len(),
                    "space_reclaimed": scope.space_reclaimed,
                })
            })
            .collect::<Vec<_>>();
        let total_reclaimed = outcome
            .receipt
            .scopes
            .iter()
            .map(|scope| scope.space_reclaimed)
            .sum::<u64>();
        let output = mutation_output(
            operation,
            outcome.changed,
            format!(
                "pruned Docker {} resources and reclaimed {} bytes",
                outcome.before.target.as_str(),
                total_reclaimed
            ),
            target.revision(),
            json!({
                "target": outcome.before.target,
                "before_fingerprint": outcome.before.sha256,
                "after_fingerprint": outcome.after.sha256,
                "before_counts": prune_counts(&outcome.before),
                "after_counts": prune_counts(&outcome.after),
                "scopes": scopes,
                "total_space_reclaimed": total_reclaimed,
            }),
        );
        self.verified_cleanup_result(
            operation,
            context,
            target,
            started,
            outcome.receipt.send_state,
            output,
            "diff",
            format!("docker-prune://{}", context.operation_id()),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn compose_down_outcome_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started: Timestamp,
        _failure_retry: RetryClass,
        outcome: ComposeDownOutcome,
    ) -> Result<OperationResult, ExecutionError> {
        let output = mutation_output(
            operation,
            outcome.changed,
            format!(
                "Compose project {} was torn down and verified empty",
                outcome.project
            ),
            Some(outcome.topology_revision.as_str()),
            json!({
                "host": outcome.host,
                "project": outcome.project,
                "services_before": outcome.before.services.iter().map(|row| row.service.clone()).collect::<Vec<_>>(),
                "services_after": outcome.after.services.len(),
                "remove_volumes": outcome.receipt.remove_volumes,
                "output_truncated": outcome.receipt.output_truncated,
                "verification": outcome.verification,
            }),
        );
        self.verified_cleanup_result(
            operation,
            context,
            target,
            started,
            outcome.receipt.send_state,
            output,
            "runtime_state",
            format!("compose-down://{}", context.operation_id()),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn verified_cleanup_result(
        &self,
        operation: &OperationName,
        context: &OperationContext,
        target: TargetRef,
        started: Timestamp,
        send_state: soma_ops::MutationSendState,
        output: Value,
        evidence_kind: &'static str,
        evidence_reference: String,
    ) -> Result<OperationResult, ExecutionError> {
        self.catalog.validate_result(operation, &output)?;
        let execution =
            ExecutionMetadata::new(started, Timestamp::now(), send_state, RetryClass::Never)?;
        let mut result = OperationResult::new(
            context.operation_id().clone(),
            operation.clone(),
            target,
            OperationStatus::Succeeded,
            execution,
        )?
        .with_output(output)?
        .with_evidence(EvidenceRef::new(evidence_kind, evidence_reference)?);
        result = result.with_verification(VerificationResult::new(
            VerificationStatus::Verified,
            Timestamp::now(),
        ))?;
        result.validate()?;
        Ok(result)
    }
}

fn prune_counts(fingerprint: &soma_infra::DockerPruneFingerprint) -> Value {
    json!({
        "containers": fingerprint.containers.len(),
        "images": fingerprint.images.len(),
        "volumes": fingerprint.volumes.len(),
        "networks": fingerprint.networks.len(),
        "build_cache_bytes": fingerprint.build_cache_bytes,
    })
}

#[cfg(test)]
#[path = "mutation_final_result_tests.rs"]
mod tests;
