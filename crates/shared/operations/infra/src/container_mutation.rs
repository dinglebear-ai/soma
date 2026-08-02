use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::{MutationSendState, Timestamp, VerificationStatus};
use tokio_util::sync::CancellationToken;

use crate::{ContainerReader, ContainerState, InfraError, MutationResult, MutationVerification};

const MAX_CONTAINER_ID_CHARS: usize = 256;
const MAX_VERIFY_ATTEMPTS: u8 = 20;
const MAX_VERIFY_INTERVAL: Duration = Duration::from_secs(5);

/// Supported reversible container lifecycle mutations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerLifecycleAction {
    /// Start a stopped container.
    Start,
    /// Stop a running container.
    Stop,
    /// Restart a container.
    Restart,
    /// Pause a running container.
    Pause,
    /// Resume a paused container.
    Resume,
}

impl ContainerLifecycleAction {
    /// Returns the canonical operation name.
    #[must_use]
    pub const fn operation_name(self) -> &'static str {
        match self {
            Self::Start => "container.start",
            Self::Stop => "container.stop",
            Self::Restart => "container.restart",
            Self::Pause => "container.pause",
            Self::Resume => "container.resume",
        }
    }

    /// Returns a stable backend-neutral action label.
    #[must_use]
    pub const fn action_label(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Pause => "pause",
            Self::Resume => "resume",
        }
    }

    pub(crate) fn already_satisfied(self, state: &ContainerState) -> bool {
        match self {
            Self::Start | Self::Resume => matches!(state, ContainerState::Running),
            Self::Stop => matches!(state, ContainerState::Exited | ContainerState::Dead),
            Self::Pause => matches!(state, ContainerState::Paused),
            Self::Restart => false,
        }
    }

    pub(crate) fn verified(self, state: &ContainerState) -> bool {
        match self {
            Self::Start | Self::Restart | Self::Resume => {
                matches!(state, ContainerState::Running)
            }
            Self::Stop => matches!(state, ContainerState::Exited | ContainerState::Dead),
            Self::Pause => matches!(state, ContainerState::Paused),
        }
    }
}

/// Deadline-bound container lifecycle request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerLifecycleRequest {
    container: String,
    action: ContainerLifecycleAction,
    deadline: Timestamp,
}

impl ContainerLifecycleRequest {
    /// Creates a validated lifecycle request.
    pub fn new(
        container: impl Into<String>,
        action: ContainerLifecycleAction,
        deadline: Timestamp,
    ) -> Result<Self, InfraError> {
        let container = container.into();
        let count = container.chars().count();
        if count == 0 || count > MAX_CONTAINER_ID_CHARS || container.chars().any(char::is_control) {
            return Err(InfraError::InvalidRequest {
                domain: "container-mutation",
                message: "invalid container identifier".into(),
            });
        }
        Ok(Self {
            container,
            action,
            deadline,
        })
    }

    /// Returns the container identifier.
    #[must_use]
    pub fn container(&self) -> &str {
        &self.container
    }

    /// Returns the lifecycle action.
    #[must_use]
    pub const fn action(&self) -> ContainerLifecycleAction {
        self.action
    }

    /// Returns the absolute request deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Bounded post-mutation verification policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MutationVerificationPolicy {
    attempts: u8,
    interval: Duration,
}

impl MutationVerificationPolicy {
    /// Creates a verification policy with explicit bounds.
    pub fn new(attempts: u8, interval: Duration) -> Result<Self, InfraError> {
        if attempts == 0 || attempts > MAX_VERIFY_ATTEMPTS || interval > MAX_VERIFY_INTERVAL {
            return Err(InfraError::InvalidRequest {
                domain: "container-mutation",
                message: format!(
                    "verification requires 1-{MAX_VERIFY_ATTEMPTS} attempts and an interval no greater than {} ms",
                    MAX_VERIFY_INTERVAL.as_millis()
                ),
            });
        }
        Ok(Self { attempts, interval })
    }

    /// Returns the attempt count.
    #[must_use]
    pub const fn attempts(self) -> u8 {
        self.attempts
    }

    /// Returns the delay between attempts.
    #[must_use]
    pub const fn interval(self) -> Duration {
        self.interval
    }
}

impl Default for MutationVerificationPolicy {
    fn default() -> Self {
        Self {
            attempts: 5,
            interval: Duration::from_millis(200),
        }
    }
}

/// Receipt returned once a lifecycle mutation was accepted by the driver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerMutationReceipt {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Container identifier.
    pub container: String,
    /// Executed action.
    pub action: ContainerLifecycleAction,
    /// Backend send state.
    pub send_state: MutationSendState,
}

/// Verified lifecycle mutation outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerLifecycleOutcome {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Container identifier.
    pub container: String,
    /// Requested action.
    pub action: ContainerLifecycleAction,
    /// Whether a backend mutation was sent.
    pub changed: bool,
    /// Mutation send state.
    pub send_state: MutationSendState,
    /// State observed before admission.
    pub before: ContainerState,
    /// Last state observed after execution.
    pub after: Option<ContainerState>,
    /// Independent verification status.
    pub verification_status: VerificationStatus,
    /// Stable verification detail.
    pub verification: MutationVerification,
}

/// Driver for one reversible container lifecycle mutation.
#[async_trait]
pub trait ContainerLifecycleMutator: Send + Sync {
    /// Sends one lifecycle mutation while preserving send uncertainty.
    async fn mutate_container(
        &self,
        host: &HostRecord,
        request: &ContainerLifecycleRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ContainerMutationReceipt>;
}

/// Complete Docker client required by the lifecycle coordinator.
pub trait DockerMutationClient: ContainerReader + ContainerLifecycleMutator {}

impl<T> DockerMutationClient for T where T: ContainerReader + ContainerLifecycleMutator {}

/// Factory for host- and revision-bound Docker mutation clients.
#[async_trait]
pub trait DockerMutationClientProvider: Send + Sync {
    /// Returns a mutation-capable client bound to the exact host revision.
    async fn mutation_client(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> Result<Arc<dyn DockerMutationClient>, InfraError>;
}

#[cfg(test)]
#[path = "container_mutation_tests.rs"]
mod tests;
