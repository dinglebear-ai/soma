use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::{MutationSendState, OperationId, OperationName, Timestamp, VerificationStatus};
use tokio_util::sync::CancellationToken;

use crate::{
    ContainerInspect, ContainerReader, ContainerState, InfraError, InfraResult, MutationResult,
    MutationVerification,
};

/// Stable digest and selected identity captured before a container replacement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerRecreateFingerprint {
    /// Original container identifier.
    pub container: String,
    /// Container name without Docker's leading slash.
    pub name: String,
    /// Configured image reference.
    pub image: String,
    /// Current runtime state.
    pub state: ContainerState,
    /// SHA-256 of replacement-relevant Docker configuration.
    pub sha256: String,
}

impl ContainerRecreateFingerprint {
    /// Creates a validated fingerprint.
    pub fn new(
        container: impl Into<String>,
        name: impl Into<String>,
        image: impl Into<String>,
        state: ContainerState,
        sha256: impl Into<String>,
    ) -> InfraResult<Self> {
        let container = container.into();
        let name = name.into();
        let image = image.into();
        let sha256 = sha256.into();
        if container.is_empty() || name.is_empty() || image.is_empty() {
            return Err(InfraError::InvalidRequest {
                domain: "container-recreate",
                message: "container, name, and image are required".into(),
            });
        }
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(InfraError::InvalidRequest {
                domain: "container-recreate",
                message: "configuration fingerprint must be SHA-256 hex".into(),
            });
        }
        Ok(Self {
            container,
            name,
            image,
            state,
            sha256: sha256.to_ascii_lowercase(),
        })
    }
}

/// Deadline-bound request to replace one container from its captured configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerRecreateRequest {
    operation_id: OperationId,
    operation: OperationName,
    expected: ContainerRecreateFingerprint,
    pull: bool,
    deadline: Timestamp,
}

impl ContainerRecreateRequest {
    /// Creates a validated recreate request.
    #[must_use]
    pub fn new(
        operation_id: OperationId,
        operation: OperationName,
        expected: ContainerRecreateFingerprint,
        pull: bool,
        deadline: Timestamp,
    ) -> Self {
        Self {
            operation_id,
            operation,
            expected,
            pull,
            deadline,
        }
    }
    /// Returns the operation identity.
    #[must_use]
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }
    /// Returns the canonical operation.
    #[must_use]
    pub fn operation(&self) -> &OperationName {
        &self.operation
    }
    /// Returns the expected pre-state fingerprint.
    #[must_use]
    pub const fn expected(&self) -> &ContainerRecreateFingerprint {
        &self.expected
    }
    /// Returns whether the image should be pulled first.
    #[must_use]
    pub const fn pull(&self) -> bool {
        self.pull
    }
    /// Returns the absolute deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Furthest destructive stage reached by a container recreation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerRecreateStage {
    /// No destructive request was sent.
    Prepared,
    /// Original container was stopped.
    Stopped,
    /// Original container was removed.
    Removed,
    /// Replacement container was created.
    Created,
    /// Replacement container was started.
    Started,
}

/// Driver receipt for one replacement attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerRecreateReceipt {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Original container identifier.
    pub original_container: String,
    /// New container identifier when creation completed.
    pub new_container: Option<String>,
    /// Captured container name.
    pub name: String,
    /// Captured image reference.
    pub image: String,
    /// Furthest stage reached.
    pub stage: ContainerRecreateStage,
    /// Backend send state.
    pub send_state: MutationSendState,
    /// Whether an image pull was requested.
    pub pulled: bool,
}

/// Verified container replacement outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerRecreateOutcome {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Captured pre-state.
    pub before: ContainerInspect,
    /// Verified post-state when available.
    pub after: Option<ContainerInspect>,
    /// Original identifier.
    pub original_container: String,
    /// Replacement identifier when created.
    pub new_container: Option<String>,
    /// Whether a replacement was observed.
    pub changed: bool,
    /// Furthest destructive stage reached.
    pub stage: ContainerRecreateStage,
    /// Whether an image pull was requested before replacement.
    pub pulled: bool,
    /// Backend send state.
    pub send_state: MutationSendState,
    /// Verification status.
    pub verification_status: VerificationStatus,
    /// Verification explanation.
    pub verification: MutationVerification,
}

/// Reads a driver-native replacement fingerprint without leaking SDK models.
#[async_trait]
pub trait ContainerRecreateInspector: Send + Sync {
    /// Captures replacement-relevant container configuration.
    async fn recreate_fingerprint(
        &self,
        host: &HostRecord,
        container: &str,
        cancellation: &CancellationToken,
    ) -> InfraResult<ContainerRecreateFingerprint>;
}

/// Performs one container replacement while preserving partial-stage evidence.
#[async_trait]
pub trait ContainerRecreateMutator: Send + Sync {
    /// Replaces a container from its captured configuration.
    async fn recreate_container(
        &self,
        host: &HostRecord,
        request: &ContainerRecreateRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ContainerRecreateReceipt>;
}

/// Complete client required by the verified recreate engine.
pub trait ContainerRecreateClient:
    ContainerReader + ContainerRecreateInspector + ContainerRecreateMutator
{
}
impl<T> ContainerRecreateClient for T where
    T: ContainerReader + ContainerRecreateInspector + ContainerRecreateMutator
{
}

/// Supplies one host-bound container replacement client.
#[async_trait]
pub trait ContainerRecreateClientProvider: Send + Sync {
    /// Creates a client bound to the exact host topology revision.
    async fn recreate_client(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> InfraResult<Arc<dyn ContainerRecreateClient>>;
}

#[cfg(test)]
#[path = "container_recreate_tests.rs"]
mod tests;
