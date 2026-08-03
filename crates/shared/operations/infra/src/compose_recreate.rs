use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::{MutationSendState, OperationId, OperationName, Timestamp, VerificationStatus};
use tokio_util::sync::CancellationToken;

use crate::{
    ComposeConfig, ComposeInspector, ComposeProjectRef, ComposeStatus, InfraError, InfraResult,
    MutationResult, MutationVerification,
};

/// Stable fingerprint of the Compose configuration and service pre-state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeRecreateFingerprint {
    /// Compose project name.
    pub project: String,
    /// Deterministic service names.
    pub services: Vec<String>,
    /// SHA-256 of normalized configuration and status material.
    pub sha256: String,
}

impl ComposeRecreateFingerprint {
    /// Creates a validated fingerprint.
    pub fn new(
        project: impl Into<String>,
        mut services: Vec<String>,
        sha256: impl Into<String>,
    ) -> InfraResult<Self> {
        let project = project.into();
        services.sort();
        services.dedup();
        let sha256 = sha256.into();
        if project.is_empty() || services.is_empty() {
            return Err(InfraError::InvalidRequest {
                domain: "compose-recreate",
                message: "project and at least one service are required".into(),
            });
        }
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(InfraError::InvalidRequest {
                domain: "compose-recreate",
                message: "Compose fingerprint must be SHA-256 hex".into(),
            });
        }
        Ok(Self {
            project,
            services,
            sha256: sha256.to_ascii_lowercase(),
        })
    }
}

/// Produces deterministic replacement material from canonical Compose reads.
pub fn compose_recreate_fingerprint(
    config: &ComposeConfig,
    status: &ComposeStatus,
) -> InfraResult<ComposeRecreateFingerprint> {
    if config.project != status.project {
        return Err(InfraError::InvalidRequest {
            domain: "compose-recreate",
            message: "Compose config and status project identities differ".into(),
        });
    }
    let mut rows = status.services.clone();
    rows.sort_by(|a, b| a.service.cmp(&b.service));
    let services = config.services.keys().cloned().collect::<Vec<_>>();
    let encoded = serde_json::to_vec(&(config, rows)).map_err(|error| InfraError::Parse {
        domain: "compose-recreate",
        message: error.to_string(),
    })?;
    ComposeRecreateFingerprint::new(
        config.project.clone(),
        services,
        format!("{:x}", Sha256::digest(encoded)),
    )
}

/// Deadline-bound Compose replacement request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeRecreateRequest {
    operation_id: OperationId,
    operation: OperationName,
    project: ComposeProjectRef,
    expected: ComposeRecreateFingerprint,
    deadline: Timestamp,
}

impl ComposeRecreateRequest {
    /// Creates a replacement request.
    #[must_use]
    pub fn new(
        operation_id: OperationId,
        operation: OperationName,
        project: ComposeProjectRef,
        expected: ComposeRecreateFingerprint,
        deadline: Timestamp,
    ) -> Self {
        Self {
            operation_id,
            operation,
            project,
            expected,
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
    /// Returns the project reference.
    #[must_use]
    pub const fn project(&self) -> &ComposeProjectRef {
        &self.project
    }
    /// Returns the expected pre-state fingerprint.
    #[must_use]
    pub const fn expected(&self) -> &ComposeRecreateFingerprint {
        &self.expected
    }
    /// Returns the deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Process driver receipt for Compose force-recreate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeRecreateReceipt {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Project name.
    pub project: String,
    /// Backend send state.
    pub send_state: MutationSendState,
    /// Bounded stdout.
    pub stdout: String,
    /// Bounded stderr.
    pub stderr: String,
    /// Whether command output was truncated.
    pub output_truncated: bool,
}

/// Verified Compose replacement outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeRecreateOutcome {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Project name.
    pub project: String,
    /// Pre-replacement status.
    pub before: ComposeStatus,
    /// Post-replacement status when readable.
    pub after: Option<ComposeStatus>,
    /// Whether a force-recreate command was sent.
    pub changed: bool,
    /// Backend send state.
    pub send_state: MutationSendState,
    /// Bounded stdout.
    pub stdout: String,
    /// Bounded stderr.
    pub stderr: String,
    /// Whether command output was truncated.
    pub output_truncated: bool,
    /// Verification status.
    pub verification_status: VerificationStatus,
    /// Verification explanation.
    pub verification: MutationVerification,
}

/// Executes Docker Compose force-recreate.
#[async_trait]
pub trait ComposeRecreateMutator: Send + Sync {
    /// Performs one force-recreate command.
    async fn recreate_compose(
        &self,
        host: &HostRecord,
        request: &ComposeRecreateRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ComposeRecreateReceipt>;
}

/// Complete Compose client required by the replacement engine.
pub trait ComposeRecreateClient: ComposeInspector + ComposeRecreateMutator {}
impl<T> ComposeRecreateClient for T where T: ComposeInspector + ComposeRecreateMutator {}

#[cfg(test)]
#[path = "compose_recreate_tests.rs"]
mod tests;
