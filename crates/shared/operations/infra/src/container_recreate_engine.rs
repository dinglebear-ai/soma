use soma_fleet::HostRecord;
use soma_ops::{MutationSendState, Timestamp, VerificationStatus};
use tokio_util::sync::CancellationToken;

use crate::{
    ContainerRecreateClient, ContainerRecreateOutcome, ContainerRecreateRequest,
    ContainerRecreateStage, ContainerState, MutationFailure, MutationResult, MutationVerification,
};

/// Coordinates configuration drift checks, replacement, and post-state verification.
#[derive(Debug, Clone, Copy, Default)]
pub struct ContainerRecreateEngine;

impl ContainerRecreateEngine {
    /// Recreates one container and verifies the replacement is running under the captured name.
    pub async fn execute(
        &self,
        client: &dyn ContainerRecreateClient,
        host: &HostRecord,
        request: &ContainerRecreateRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ContainerRecreateOutcome> {
        ensure_admitted(request, cancellation)?;
        let current = client
            .recreate_fingerprint(host, &request.expected().container, cancellation)
            .await
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        if current != *request.expected() {
            return Err(MutationFailure::new(
                MutationSendState::NotSent,
                crate::InfraError::InvalidRequest {
                    domain: "container-recreate",
                    message: "container configuration changed after planning".into(),
                },
            ));
        }
        let before = client
            .inspect_container(host, &request.expected().container, cancellation)
            .await
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        let receipt = client
            .recreate_container(host, request, cancellation)
            .await?;
        let Some(new_id) = receipt.new_container.clone() else {
            return Ok(ContainerRecreateOutcome {
                host: host.id().clone(),
                topology_revision: host.revision().clone(),
                before,
                after: None,
                original_container: receipt.original_container,
                new_container: None,
                changed: receipt.stage != ContainerRecreateStage::Prepared,
                stage: receipt.stage,
                pulled: receipt.pulled,
                send_state: receipt.send_state,
                verification_status: VerificationStatus::Failed,
                verification: MutationVerification {
                    status: "failed".into(),
                    summary: "replacement did not produce a new container identifier".into(),
                },
            });
        };
        let after_read = client.inspect_container(host, &new_id, cancellation).await;
        let (after, verification_status, summary) = match after_read {
            Ok(after) => {
                let actual_name = after
                    .name
                    .as_deref()
                    .unwrap_or_default()
                    .trim_start_matches('/');
                if actual_name == request.expected().name && after.state == ContainerState::Running
                {
                    (
                        Some(after),
                        VerificationStatus::Verified,
                        "replacement container is running under the captured name".into(),
                    )
                } else {
                    (
                        Some(after),
                        VerificationStatus::Failed,
                        "replacement container did not reach the captured running post-state"
                            .into(),
                    )
                }
            }
            Err(error) => (
                None,
                VerificationStatus::Inconclusive,
                format!("replacement was created but post-state inspection failed: {error}"),
            ),
        };
        Ok(ContainerRecreateOutcome {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            before,
            after,
            original_container: receipt.original_container,
            new_container: Some(new_id),
            changed: true,
            stage: receipt.stage,
            pulled: receipt.pulled,
            send_state: receipt.send_state,
            verification_status,
            verification: MutationVerification {
                status: format!("{verification_status:?}").to_ascii_lowercase(),
                summary,
            },
        })
    }
}

fn ensure_admitted(
    request: &ContainerRecreateRequest,
    cancellation: &CancellationToken,
) -> MutationResult<()> {
    if cancellation.is_cancelled() {
        return Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::Cancelled.into(),
        ));
    }
    if Timestamp::now() >= request.deadline() {
        return Err(MutationFailure::new(
            MutationSendState::NotSent,
            soma_fleet::FleetError::DeadlineExceeded.into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "container_recreate_engine_tests.rs"]
mod tests;
