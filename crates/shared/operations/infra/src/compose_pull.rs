use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::{MutationSendState, OperationId, OperationName, Timestamp, VerificationStatus};
use tokio_util::sync::CancellationToken;

use crate::{
    ComposeInspector, ComposeProjectRef, ImageIdentity, InfraError, MutationProgressReporter,
    MutationResult, MutationVerification,
};

const MAX_SERVICE_CHARS: usize = 128;

/// Deadline-bound Compose image pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposePullRequest {
    operation_id: OperationId,
    operation: OperationName,
    project: ComposeProjectRef,
    service: Option<String>,
    deadline: Timestamp,
}

impl ComposePullRequest {
    /// Creates a validated Compose pull request.
    pub fn new(
        operation_id: OperationId,
        operation: OperationName,
        project: ComposeProjectRef,
        service: Option<String>,
        deadline: Timestamp,
    ) -> Result<Self, InfraError> {
        if let Some(service) = &service {
            validate_service(service)?;
        }
        Ok(Self {
            operation_id,
            operation,
            project,
            service,
            deadline,
        })
    }

    /// Returns the operation identity.
    #[must_use]
    pub fn operation_id(&self) -> &OperationId {
        &self.operation_id
    }
    /// Returns the canonical operation name.
    #[must_use]
    pub fn operation(&self) -> &OperationName {
        &self.operation
    }
    /// Returns the Compose project.
    #[must_use]
    pub const fn project(&self) -> &ComposeProjectRef {
        &self.project
    }
    /// Returns the optional service filter.
    #[must_use]
    pub fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }
    /// Returns the absolute deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Receipt returned when the Compose pull command completes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposePullReceipt {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Project name.
    pub project: String,
    /// Optional service filter.
    pub service: Option<String>,
    /// Backend send state.
    pub send_state: MutationSendState,
    /// Bounded progress delivery failures.
    pub progress_delivery_errors: Vec<String>,
    /// Whether command output was truncated.
    pub output_truncated: bool,
}

/// One Compose service image verification row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposePulledImage {
    /// Compose service name.
    pub service: String,
    /// Configured image reference.
    pub reference: String,
    /// Image identity before the pull.
    pub before: Option<ImageIdentity>,
    /// Image identity after the pull.
    pub after: Option<ImageIdentity>,
    /// Whether the verified image content identity changed.
    pub changed: bool,
    /// Whether the configured reference resolved locally after the pull.
    pub verified: bool,
}

/// Verified Compose pull outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposePullOutcome {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Compose project name.
    pub project: String,
    /// Optional service filter.
    pub service: Option<String>,
    /// Backend send state.
    pub send_state: MutationSendState,
    /// Per-service image verification.
    pub images: Vec<ComposePulledImage>,
    /// Whether any verified image identity changed.
    pub changed: bool,
    /// Progress sink failures that did not alter execution truth.
    pub progress_delivery_errors: Vec<String>,
    /// Whether command output was truncated.
    pub output_truncated: bool,
    /// Independent verification status.
    pub verification_status: VerificationStatus,
    /// Verification explanation.
    pub verification: MutationVerification,
}

/// Driver for one Compose image pull command.
#[async_trait]
pub trait ComposePullMutator: Send + Sync {
    /// Pulls configured service images while preserving send uncertainty.
    async fn pull_compose_images(
        &self,
        host: &HostRecord,
        request: &ComposePullRequest,
        progress: &dyn MutationProgressReporter,
        cancellation: &CancellationToken,
    ) -> MutationResult<ComposePullReceipt>;
}

/// Complete Compose client required by the pull coordinator.
pub trait ComposePullClient: ComposeInspector + ComposePullMutator {}
impl<T> ComposePullClient for T where T: ComposeInspector + ComposePullMutator {}

pub(crate) fn validate_service(service: &str) -> Result<(), InfraError> {
    let count = service.chars().count();
    if count == 0
        || count > MAX_SERVICE_CHARS
        || service.starts_with('-')
        || !service
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        Err(InfraError::InvalidRequest {
            domain: "compose-pull",
            message: "invalid Compose service name".into(),
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "compose_pull_tests.rs"]
mod tests;
