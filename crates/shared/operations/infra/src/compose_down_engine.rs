use soma_fleet::HostRecord;
use soma_ops::{MutationSendState, VerificationStatus};
use tokio_util::sync::CancellationToken;

use crate::{
    ComposeDownClient, ComposeDownOutcome, ComposeDownRequest, ComposeRecreateFingerprint,
    InfraError, MutationFailure, MutationResult, MutationVerification,
    compose_recreate_fingerprint,
};

/// Verified Docker Compose teardown coordinator.
#[derive(Debug, Clone, Copy, Default)]
pub struct ComposeDownEngine;

impl ComposeDownEngine {
    /// Captures the current normalized Compose config and status fingerprint.
    pub async fn inspect(
        &self,
        client: &dyn ComposeDownClient,
        host: &HostRecord,
        project: &crate::ComposeProjectRef,
        deadline: soma_ops::Timestamp,
        cancellation: &CancellationToken,
    ) -> crate::InfraResult<(ComposeRecreateFingerprint, crate::ComposeStatus)> {
        let config = client.config(host, project, deadline, cancellation).await?;
        let status = client
            .status(host, project, None, deadline, cancellation)
            .await?;
        let fingerprint = compose_recreate_fingerprint(&config, &status)?;
        Ok((fingerprint, status))
    }

    /// Executes and independently verifies Compose teardown.
    pub async fn execute(
        &self,
        client: &dyn ComposeDownClient,
        host: &HostRecord,
        request: &ComposeDownRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ComposeDownOutcome> {
        admit(request, cancellation)?;
        let (current, before) = self
            .inspect(
                client,
                host,
                request.project(),
                request.deadline(),
                cancellation,
            )
            .await
            .map_err(not_sent)?;
        if current != *request.expected() {
            return Err(not_sent(InfraError::InvalidRequest {
                domain: "compose-down",
                message: "Compose config or service state changed after planning".into(),
            }));
        }
        let receipt = client.down_compose(host, request, cancellation).await?;
        let after = client
            .status(
                host,
                request.project(),
                None,
                request.deadline(),
                cancellation,
            )
            .await
            .map_err(|error| MutationFailure::new(receipt.send_state, error))?;
        if !after.services.is_empty() {
            return Err(MutationFailure::new(
                receipt.send_state,
                InfraError::InvalidRequest {
                    domain: "compose-down",
                    message: "Compose services remain after down".into(),
                },
            ));
        }
        Ok(ComposeDownOutcome {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: request.project().name().to_owned(),
            changed: !before.services.is_empty() || request.remove_volumes(),
            before,
            after,
            receipt,
            verification_status: VerificationStatus::Verified,
            verification: MutationVerification {
                status: "verified".into(),
                summary: "Compose status reports no remaining services".into(),
            },
        })
    }
}

fn admit(request: &ComposeDownRequest, cancellation: &CancellationToken) -> MutationResult<()> {
    if request.remove_volumes() && !request.force() {
        return Err(not_sent(InfraError::InvalidRequest {
            domain: "compose-down",
            message: "remove_volumes=true requires force=true".into(),
        }));
    }
    if cancellation.is_cancelled() {
        return Err(not_sent(soma_fleet::FleetError::Cancelled.into()));
    }
    if request.deadline() <= soma_ops::Timestamp::now() {
        return Err(not_sent(soma_fleet::FleetError::DeadlineExceeded.into()));
    }
    Ok(())
}

fn not_sent(error: InfraError) -> MutationFailure {
    MutationFailure::new(MutationSendState::NotSent, error)
}

#[cfg(test)]
#[path = "compose_down_engine_tests.rs"]
mod tests;
