use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::{MutationSendState, Timestamp, VerificationStatus};
use tokio_util::sync::CancellationToken;

use crate::{
    ComposeInspector, ComposeProjectRef, ComposeStatus, MutationFailure, MutationResult,
    MutationVerification, MutationVerificationPolicy,
};

/// Supported Compose mutations in the first reversible slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComposeMutationAction {
    /// Create or reconcile project services in detached mode.
    Up,
    /// Restart existing project services.
    Restart,
}

impl ComposeMutationAction {
    /// Returns the canonical operation name.
    #[must_use]
    pub const fn operation_name(self) -> &'static str {
        match self {
            Self::Up => "compose.up",
            Self::Restart => "compose.restart",
        }
    }

    /// Returns the Compose CLI action.
    #[must_use]
    pub const fn action_label(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Restart => "restart",
        }
    }
}

/// Deadline-bound Compose mutation request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeMutationRequest {
    project: ComposeProjectRef,
    action: ComposeMutationAction,
    deadline: Timestamp,
}

impl ComposeMutationRequest {
    /// Creates a Compose mutation request.
    #[must_use]
    pub const fn new(
        project: ComposeProjectRef,
        action: ComposeMutationAction,
        deadline: Timestamp,
    ) -> Self {
        Self {
            project,
            action,
            deadline,
        }
    }

    /// Returns the project reference.
    #[must_use]
    pub const fn project(&self) -> &ComposeProjectRef {
        &self.project
    }

    /// Returns the mutation action.
    #[must_use]
    pub const fn action(&self) -> ComposeMutationAction {
        self.action
    }

    /// Returns the absolute deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Receipt returned after a Compose mutation command was sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeMutationReceipt {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Project name.
    pub project: String,
    /// Executed action.
    pub action: ComposeMutationAction,
    /// Mutation send state.
    pub send_state: MutationSendState,
}

/// Verified Compose mutation outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeMutationOutcome {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Project name.
    pub project: String,
    /// Requested action.
    pub action: ComposeMutationAction,
    /// Mutation send state.
    pub send_state: MutationSendState,
    /// Status observed before mutation, when available.
    pub before: Option<ComposeStatus>,
    /// Last status observed after mutation.
    pub after: Option<ComposeStatus>,
    /// Independent verification status.
    pub verification_status: VerificationStatus,
    /// Verification explanation.
    pub verification: MutationVerification,
}

/// Driver for Compose mutation commands.
#[async_trait]
pub trait ComposeMutator: Send + Sync {
    /// Sends one Compose mutation while preserving send uncertainty.
    async fn mutate_compose(
        &self,
        host: &HostRecord,
        request: &ComposeMutationRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ComposeMutationReceipt>;
}

/// Complete client required by the Compose mutation coordinator.
pub trait ComposeMutationClient: ComposeInspector + ComposeMutator {}

impl<T> ComposeMutationClient for T where T: ComposeInspector + ComposeMutator {}

/// Coordinates a Compose mutation and independent service-state verification.
#[derive(Debug, Clone, Copy)]
pub struct ComposeMutationEngine {
    verification: MutationVerificationPolicy,
}

impl ComposeMutationEngine {
    /// Creates an engine using the supplied verification policy.
    #[must_use]
    pub const fn new(verification: MutationVerificationPolicy) -> Self {
        Self { verification }
    }

    /// Sends and verifies one Compose mutation.
    pub async fn execute(
        &self,
        client: &dyn ComposeMutationClient,
        host: &HostRecord,
        request: &ComposeMutationRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ComposeMutationOutcome> {
        ensure_admitted(request, cancellation)?;
        let before = client
            .status(
                host,
                request.project(),
                None,
                request.deadline(),
                cancellation,
            )
            .await
            .ok();
        let receipt = client.mutate_compose(host, request, cancellation).await?;
        let verification = self.verify(client, host, request, cancellation).await;
        Ok(ComposeMutationOutcome {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: request.project().name().to_owned(),
            action: request.action(),
            send_state: receipt.send_state,
            before,
            after: verification.status,
            verification_status: verification.verification_status,
            verification: MutationVerification {
                status: verification_status_text(verification.verification_status),
                summary: verification.summary,
            },
        })
    }

    async fn verify(
        &self,
        client: &dyn ComposeMutationClient,
        host: &HostRecord,
        request: &ComposeMutationRequest,
        cancellation: &CancellationToken,
    ) -> ComposeVerificationObservation {
        let mut last_status = None;
        let mut last_error = None;
        for attempt in 0..self.verification.attempts() {
            if cancellation.is_cancelled() {
                return ComposeVerificationObservation::inconclusive(
                    last_status,
                    "Compose verification cancelled after mutation send",
                );
            }
            if Timestamp::now() >= request.deadline() {
                return ComposeVerificationObservation::inconclusive(
                    last_status,
                    "Compose verification deadline expired after mutation send",
                );
            }
            match client
                .status(
                    host,
                    request.project(),
                    None,
                    request.deadline(),
                    cancellation,
                )
                .await
            {
                Ok(status) => {
                    if compose_status_running(&status) {
                        return ComposeVerificationObservation {
                            status: Some(status),
                            verification_status: VerificationStatus::Verified,
                            summary: "all reported Compose services are running".into(),
                        };
                    }
                    last_status = Some(status);
                }
                Err(error) => last_error = Some(error.to_string()),
            }
            if attempt + 1 < self.verification.attempts() {
                tokio::select! {
                    () = cancellation.cancelled() => {
                        return ComposeVerificationObservation::inconclusive(
                            last_status,
                            "Compose verification cancelled after mutation send",
                        );
                    }
                    () = tokio::time::sleep(self.verification.interval()) => {}
                }
            }
        }
        let summary = last_error.map_or_else(
            || "one or more Compose services did not reach running state".into(),
            |error| format!("Compose status could not be verified: {error}"),
        );
        ComposeVerificationObservation {
            status: last_status,
            verification_status: VerificationStatus::Failed,
            summary,
        }
    }
}

impl Default for ComposeMutationEngine {
    fn default() -> Self {
        Self::new(MutationVerificationPolicy::default())
    }
}

struct ComposeVerificationObservation {
    status: Option<ComposeStatus>,
    verification_status: VerificationStatus,
    summary: String,
}

impl ComposeVerificationObservation {
    fn inconclusive(status: Option<ComposeStatus>, summary: &str) -> Self {
        Self {
            status,
            verification_status: VerificationStatus::Inconclusive,
            summary: summary.into(),
        }
    }
}

fn ensure_admitted(
    request: &ComposeMutationRequest,
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

fn compose_status_running(status: &ComposeStatus) -> bool {
    !status.services.is_empty()
        && status.services.iter().all(|service| {
            service
                .state
                .as_deref()
                .is_some_and(|state| state.eq_ignore_ascii_case("running"))
                && !service.health.as_deref().is_some_and(|health| {
                    matches!(
                        health.to_ascii_lowercase().as_str(),
                        "unhealthy" | "starting"
                    )
                })
                && service.exit_code.unwrap_or(0) == 0
        })
}

fn verification_status_text(status: VerificationStatus) -> String {
    format!("{status:?}").to_ascii_lowercase()
}

#[cfg(test)]
#[path = "compose_mutation_tests.rs"]
mod tests;
