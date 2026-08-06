use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::{MutationSendState, OperationId, OperationName, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::{
    ContainerReader, DockerTelemetryReader, ImageIdentity, ImageReader, InfraError, InfraResult,
    MutationResult, NetworkReader, VolumeReader,
};

/// Closed Docker prune scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockerPruneTarget {
    /// Stopped containers.
    Containers,
    /// Dangling images.
    Images,
    /// Unused volumes.
    Volumes,
    /// Unused networks.
    Networks,
    /// Build cache.
    BuildCache,
    /// Every supported prune scope in a fixed order.
    All,
}

impl DockerPruneTarget {
    /// Parses the canonical schema value.
    pub fn parse(value: &str) -> InfraResult<Self> {
        match value {
            "containers" => Ok(Self::Containers),
            "images" => Ok(Self::Images),
            "volumes" => Ok(Self::Volumes),
            "networks" => Ok(Self::Networks),
            "buildcache" => Ok(Self::BuildCache),
            "all" => Ok(Self::All),
            _ => Err(InfraError::InvalidRequest {
                domain: "docker-cleanup",
                message: format!("unsupported prune target: {value}"),
            }),
        }
    }

    /// Returns the canonical label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Containers => "containers",
            Self::Images => "images",
            Self::Volumes => "volumes",
            Self::Networks => "networks",
            Self::BuildCache => "buildcache",
            Self::All => "all",
        }
    }

    #[cfg(any(feature = "bollard-driver", test))]
    pub(crate) fn expanded(self) -> &'static [Self] {
        match self {
            Self::All => &[
                Self::Containers,
                Self::Images,
                Self::Volumes,
                Self::Networks,
                Self::BuildCache,
            ],
            Self::Containers => &[Self::Containers],
            Self::Images => &[Self::Images],
            Self::Volumes => &[Self::Volumes],
            Self::Networks => &[Self::Networks],
            Self::BuildCache => &[Self::BuildCache],
        }
    }
}

/// Stable identity bound into an image-removal plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageRemovalFingerprint {
    /// Requested reference.
    pub reference: String,
    /// Resolved local image identity.
    pub identity: ImageIdentity,
    /// Lowercase SHA-256 over the complete identity.
    pub sha256: String,
}

impl ImageRemovalFingerprint {
    /// Builds a deterministic fingerprint.
    pub fn new(reference: impl Into<String>, mut identity: ImageIdentity) -> InfraResult<Self> {
        let reference = validate_text("image reference", reference.into(), 256)?;
        identity.repo_tags.sort();
        identity.repo_tags.dedup();
        identity.repo_digests.sort();
        identity.repo_digests.dedup();
        let material = serde_json::to_vec(&(reference.as_str(), &identity)).map_err(|error| {
            InfraError::Parse {
                domain: "docker-cleanup",
                message: error.to_string(),
            }
        })?;
        let sha256 = crate::mutation::sha256_hex(&material);
        Ok(Self {
            reference,
            identity,
            sha256,
        })
    }
}

/// Deterministic pre-prune inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockerPruneFingerprint {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Requested prune target.
    pub target: DockerPruneTarget,
    /// Candidate container IDs.
    pub containers: Vec<String>,
    /// Candidate image IDs.
    pub images: Vec<String>,
    /// Visible volume names.
    pub volumes: Vec<String>,
    /// Visible network IDs or names.
    pub networks: Vec<String>,
    /// Current build-cache bytes when reported.
    pub build_cache_bytes: u64,
    /// Lowercase SHA-256 over the inventory.
    pub sha256: String,
}

impl DockerPruneFingerprint {
    pub(crate) fn finalize(mut self) -> InfraResult<Self> {
        for values in [
            &mut self.containers,
            &mut self.images,
            &mut self.volumes,
            &mut self.networks,
        ] {
            values.sort();
            values.dedup();
        }
        let material = serde_json::to_vec(&(
            &self.host,
            &self.topology_revision,
            self.target,
            &self.containers,
            &self.images,
            &self.volumes,
            &self.networks,
            self.build_cache_bytes,
        ))
        .map_err(|error| InfraError::Parse {
            domain: "docker-cleanup",
            message: error.to_string(),
        })?;
        self.sha256 = crate::mutation::sha256_hex(&material);
        Ok(self)
    }
}

/// Request to remove one exact local image identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRemovalRequest {
    /// Operation identity.
    pub operation_id: OperationId,
    /// Canonical operation.
    pub operation: OperationName,
    /// Planned image fingerprint.
    pub fingerprint: ImageRemovalFingerprint,
    /// Explicit destructive confirmation field.
    pub force: bool,
    /// Absolute execution deadline.
    pub deadline: Timestamp,
}

/// Request to prune one exact inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerPruneRequest {
    /// Operation identity.
    pub operation_id: OperationId,
    /// Canonical operation.
    pub operation: OperationName,
    /// Planned prune fingerprint.
    pub fingerprint: DockerPruneFingerprint,
    /// Explicit destructive confirmation field.
    pub force: bool,
    /// Absolute execution deadline.
    pub deadline: Timestamp,
}

/// Backend receipt for image removal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageRemovalReceipt {
    /// Backend send state.
    pub send_state: MutationSendState,
    /// Deleted content IDs reported by Docker.
    pub deleted: Vec<String>,
    /// Untagged references reported by Docker.
    pub untagged: Vec<String>,
}

/// Backend receipt for one prune scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockerPruneScopeReceipt {
    /// Prune scope.
    pub target: DockerPruneTarget,
    /// Deleted object identities.
    pub deleted: Vec<String>,
    /// Reclaimed bytes reported by Docker.
    pub space_reclaimed: u64,
}

/// Complete prune receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockerPruneReceipt {
    /// Backend send state.
    pub send_state: MutationSendState,
    /// Completed scopes in execution order.
    pub scopes: Vec<DockerPruneScopeReceipt>,
}

/// Verified image-removal result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageRemovalOutcome {
    /// Planned image identity.
    pub before: ImageRemovalFingerprint,
    /// Whether the image is absent after execution.
    pub removed: bool,
    /// Backend receipt.
    pub receipt: ImageRemovalReceipt,
}

/// Verified prune result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockerPruneOutcome {
    /// Planned inventory.
    pub before: DockerPruneFingerprint,
    /// Post-prune inventory.
    pub after: DockerPruneFingerprint,
    /// Backend receipt.
    pub receipt: DockerPruneReceipt,
    /// Whether any identity or bytes were reported removed.
    pub changed: bool,
}

/// Product-neutral Docker cleanup mutations.
#[async_trait]
pub trait DockerCleanupMutator: Send + Sync {
    /// Removes one image.
    async fn remove_image(
        &self,
        host: &HostRecord,
        request: &ImageRemovalRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<ImageRemovalReceipt>;

    /// Prunes one target scope.
    async fn prune(
        &self,
        host: &HostRecord,
        request: &DockerPruneRequest,
        cancellation: &CancellationToken,
    ) -> MutationResult<DockerPruneReceipt>;
}

/// Complete Docker cleanup client used by verification engines.
pub trait DockerCleanupClient:
    ImageReader
    + ContainerReader
    + NetworkReader
    + VolumeReader
    + DockerTelemetryReader
    + DockerCleanupMutator
{
}

impl<T> DockerCleanupClient for T where
    T: ImageReader
        + ContainerReader
        + NetworkReader
        + VolumeReader
        + DockerTelemetryReader
        + DockerCleanupMutator
{
}

/// Host-bound cleanup client provider.
#[async_trait]
pub trait DockerCleanupClientProvider: Send + Sync {
    /// Resolves one cleanup client for the exact host revision.
    async fn cleanup_client(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> InfraResult<Arc<dyn DockerCleanupClient>>;
}

fn validate_text(field: &'static str, value: String, max: usize) -> InfraResult<String> {
    let count = value.chars().count();
    if count == 0 || count > max || value.chars().any(char::is_control) {
        Err(InfraError::InvalidRequest {
            domain: "docker-cleanup",
            message: format!("invalid {field}"),
        })
    } else {
        Ok(value)
    }
}

#[cfg(test)]
#[path = "docker_cleanup_tests.rs"]
mod tests;
