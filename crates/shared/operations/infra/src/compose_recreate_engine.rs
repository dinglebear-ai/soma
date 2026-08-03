use soma_fleet::HostRecord;
use soma_ops::{MutationSendState, Timestamp, VerificationStatus};
use tokio_util::sync::CancellationToken;

use crate::{
    ComposeRecreateClient, ComposeRecreateOutcome, ComposeRecreateRequest, MutationFailure,
    MutationResult, MutationVerification, compose_recreate_fingerprint,
};

/// Coordinates Compose drift checks, force-recreate, and post-state verification.
#[derive(Debug, Clone, Copy, Default)]
pub struct ComposeRecreateEngine;

impl ComposeRecreateEngine {
    /// Recreates a Compose project and verifies every configured service is running.
    pub async fn execute(
        &self,
        client: &dyn ComposeRecreateClient,
        host: &HostRecord,
        request: &ComposeRecreateRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ComposeRecreateOutcome> {
        ensure_admitted(request, cancellation)?;
        let config = client
            .config(host, request.project(), request.deadline(), cancellation)
            .await
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        let before = client
            .status(
                host,
                request.project(),
                None,
                request.deadline(),
                cancellation,
            )
            .await
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        let current = compose_recreate_fingerprint(&config, &before)
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?;
        if current != *request.expected() {
            return Err(MutationFailure::new(
                MutationSendState::NotSent,
                crate::InfraError::InvalidRequest {
                    domain: "compose-recreate",
                    message: "Compose configuration or service pre-state changed after planning"
                        .into(),
                },
            ));
        }
        let receipt = client.recreate_compose(host, request, cancellation).await?;
        let after_read = client
            .status(
                host,
                request.project(),
                None,
                request.deadline(),
                cancellation,
            )
            .await;
        let (after, verification_status, summary) = match after_read {
            Ok(after) => {
                let mut observed = after
                    .services
                    .iter()
                    .map(|service| service.service.clone())
                    .collect::<Vec<_>>();
                observed.sort();
                observed.dedup();
                let healthy = after.services.iter().all(|service| {
                    service
                        .state
                        .as_deref()
                        .is_some_and(|state| state.eq_ignore_ascii_case("running"))
                        && service.health.as_deref().is_none_or(|health| {
                            health.eq_ignore_ascii_case("healthy") || health.is_empty()
                        })
                        && service.exit_code.unwrap_or(0) == 0
                });
                if observed == request.expected().services && healthy {
                    (
                        Some(after),
                        VerificationStatus::Verified,
                        "all configured Compose services are running after force-recreate".into(),
                    )
                } else {
                    (
                        Some(after),
                        VerificationStatus::Failed,
                        "Compose force-recreate completed without the expected healthy service set"
                            .into(),
                    )
                }
            }
            Err(error) => (
                None,
                VerificationStatus::Inconclusive,
                format!("Compose force-recreate completed but status verification failed: {error}"),
            ),
        };
        Ok(ComposeRecreateOutcome {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: request.project().name().to_owned(),
            before,
            after,
            changed: true,
            send_state: receipt.send_state,
            stdout: receipt.stdout,
            stderr: receipt.stderr,
            output_truncated: receipt.output_truncated,
            verification_status,
            verification: MutationVerification {
                status: format!("{verification_status:?}").to_ascii_lowercase(),
                summary,
            },
        })
    }
}

fn ensure_admitted(
    request: &ComposeRecreateRequest,
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
#[path = "compose_recreate_engine_tests.rs"]
mod tests;
