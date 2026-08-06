use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::{MutationSendState, OperationId, OperationName, Timestamp, VerificationStatus};
use tokio_util::sync::CancellationToken;

use crate::{
    ComposeInspector, ComposeProjectRef, ComposeRecreateFingerprint, ComposeStatus, InfraError,
    InfraResult, MutationResult, MutationVerification,
};

/// Deadline-bound Compose teardown request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeDownRequest {
    operation_id: OperationId,
    operation: OperationName,
    project: ComposeProjectRef,
    expected: ComposeRecreateFingerprint,
    force: bool,
    remove_volumes: bool,
    deadline: Timestamp,
}

impl ComposeDownRequest {
    /// Creates a validated teardown request.
    pub fn new(
        operation_id: OperationId,
        operation: OperationName,
        project: ComposeProjectRef,
        expected: ComposeRecreateFingerprint,
        force: bool,
        remove_volumes: bool,
        deadline: Timestamp,
    ) -> InfraResult<Self> {
        if remove_volumes && !force {
            return Err(InfraError::InvalidRequest {
                domain: "compose-down",
                message: "remove_volumes=true requires force=true".into(),
            });
        }
        Ok(Self {
            operation_id,
            operation,
            project,
            expected,
            force,
            remove_volumes,
            deadline,
        })
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
    /// Returns the Compose project reference.
    #[must_use]
    pub const fn project(&self) -> &ComposeProjectRef {
        &self.project
    }
    /// Returns the expected pre-state.
    #[must_use]
    pub const fn expected(&self) -> &ComposeRecreateFingerprint {
        &self.expected
    }
    /// Returns explicit force confirmation.
    #[must_use]
    pub const fn force(&self) -> bool {
        self.force
    }
    /// Returns whether named volumes are removed.
    #[must_use]
    pub const fn remove_volumes(&self) -> bool {
        self.remove_volumes
    }
    /// Returns the execution deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Process driver receipt for Compose down.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeDownReceipt {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Project name.
    pub project: String,
    /// Whether volume deletion was requested.
    pub remove_volumes: bool,
    /// Backend send state.
    pub send_state: MutationSendState,
    /// Bounded stdout.
    pub stdout: String,
    /// Bounded stderr.
    pub stderr: String,
    /// Whether output was truncated.
    pub output_truncated: bool,
}

/// Verified Compose teardown outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeDownOutcome {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Project name.
    pub project: String,
    /// Service state before teardown.
    pub before: ComposeStatus,
    /// Service state after teardown.
    pub after: ComposeStatus,
    /// Whether a nonempty service set or volume deletion was requested.
    pub changed: bool,
    /// Backend receipt.
    pub receipt: ComposeDownReceipt,
    /// Verification status.
    pub verification_status: VerificationStatus,
    /// Verification explanation.
    pub verification: MutationVerification,
}

/// Executes Docker Compose teardown.
#[async_trait]
pub trait ComposeDownMutator: Send + Sync {
    /// Performs one shell-free Compose down command.
    async fn down_compose(
        &self,
        host: &HostRecord,
        request: &ComposeDownRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ComposeDownReceipt>;
}

/// Complete Compose client required by teardown verification.
pub trait ComposeDownClient: ComposeInspector + ComposeDownMutator {}
impl<T> ComposeDownClient for T where T: ComposeInspector + ComposeDownMutator {}

#[cfg(test)]
#[path = "compose_down_tests.rs"]
mod tests;
