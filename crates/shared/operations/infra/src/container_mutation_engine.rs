use soma_fleet::HostRecord;
use soma_ops::{MutationSendState, Timestamp, VerificationStatus};
use tokio_util::sync::CancellationToken;

use crate::{
    ContainerLifecycleOutcome, ContainerLifecycleRequest, ContainerState, DockerMutationClient,
    MutationFailure, MutationResult, MutationVerification, MutationVerificationPolicy,
};

/// Coordinates mutation and independent container-state verification.
#[derive(Debug, Clone, Copy)]
pub struct ContainerLifecycleEngine {
    verification: MutationVerificationPolicy,
}

impl ContainerLifecycleEngine {
    /// Creates an engine using the supplied verification policy.
    #[must_use]
    pub const fn new(verification: MutationVerificationPolicy) -> Self {
        Self { verification }
    }

    /// Executes and verifies one lifecycle mutation.
    pub async fn execute(
        &self,
        client: &dyn DockerMutationClient,
        host: &HostRecord,
        request: &ContainerLifecycleRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ContainerLifecycleOutcome> {
        ensure_admitted(request, cancellation)?;
        let before = client
            .inspect_container(host, request.container(), cancellation)
            .await
            .map_err(|error| MutationFailure::new(MutationSendState::NotSent, error))?
            .state;

        if request.action().already_satisfied(&before) {
            return Ok(outcome(
                host,
                request,
                false,
                MutationSendState::NotSent,
                before.clone(),
                Some(before),
                VerificationStatus::Verified,
                "requested state was already satisfied",
            ));
        }

        let receipt = client.mutate_container(host, request, cancellation).await?;
        let verification = self.verify(client, host, request, cancellation).await;
        Ok(outcome(
            host,
            request,
            true,
            receipt.send_state,
            before,
            verification.state,
            verification.status,
            verification.summary,
        ))
    }

    async fn verify(
        &self,
        client: &dyn DockerMutationClient,
        host: &HostRecord,
        request: &ContainerLifecycleRequest,
        cancellation: &CancellationToken,
    ) -> VerificationObservation {
        let mut last_state = None;
        let mut last_error = None;
        for attempt in 0..self.verification.attempts() {
            if cancellation.is_cancelled() {
                return VerificationObservation::inconclusive(
                    last_state,
                    "verification cancelled after mutation send",
                );
            }
            if Timestamp::now() >= request.deadline() {
                return VerificationObservation::inconclusive(
                    last_state,
                    "verification deadline expired after mutation send",
                );
            }
            match client
                .inspect_container(host, request.container(), cancellation)
                .await
            {
                Ok(inspect) => {
                    last_state = Some(inspect.state);
                    if last_state
                        .as_ref()
                        .is_some_and(|state| request.action().verified(state))
                    {
                        return VerificationObservation {
                            state: last_state,
                            status: VerificationStatus::Verified,
                            summary: "runtime state matches the requested lifecycle state".into(),
                        };
                    }
                }
                Err(error) => last_error = Some(error.to_string()),
            }
            if attempt + 1 < self.verification.attempts() {
                tokio::select! {
                    () = cancellation.cancelled() => {
                        return VerificationObservation::inconclusive(
                            last_state,
                            "verification cancelled after mutation send",
                        );
                    }
                    () = tokio::time::sleep(self.verification.interval()) => {}
                }
            }
        }
        let summary = last_error.map_or_else(
            || "runtime state did not reach the requested lifecycle state".into(),
            |error| format!("runtime state could not be verified: {error}"),
        );
        VerificationObservation {
            state: last_state,
            status: VerificationStatus::Failed,
            summary,
        }
    }
}

impl Default for ContainerLifecycleEngine {
    fn default() -> Self {
        Self::new(MutationVerificationPolicy::default())
    }
}

struct VerificationObservation {
    state: Option<ContainerState>,
    status: VerificationStatus,
    summary: String,
}

impl VerificationObservation {
    fn inconclusive(state: Option<ContainerState>, summary: &str) -> Self {
        Self {
            state,
            status: VerificationStatus::Inconclusive,
            summary: summary.into(),
        }
    }
}

fn ensure_admitted(
    request: &ContainerLifecycleRequest,
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

#[allow(clippy::too_many_arguments)]
fn outcome(
    host: &HostRecord,
    request: &ContainerLifecycleRequest,
    changed: bool,
    send_state: MutationSendState,
    before: ContainerState,
    after: Option<ContainerState>,
    verification_status: VerificationStatus,
    summary: impl Into<String>,
) -> ContainerLifecycleOutcome {
    ContainerLifecycleOutcome {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        container: request.container().to_owned(),
        action: request.action(),
        changed,
        send_state,
        before,
        after,
        verification_status,
        verification: MutationVerification {
            status: format!("{verification_status:?}").to_ascii_lowercase(),
            summary: summary.into(),
        },
    }
}

#[cfg(test)]
#[path = "container_mutation_engine_tests.rs"]
mod tests;
