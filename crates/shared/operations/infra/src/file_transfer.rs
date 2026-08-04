use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{FileTransfer, HostId, HostRecord, TopologyRevision, TransferReceipt};
use soma_ops::{MutationSendState, OperationId, OperationName, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::{FileReadPolicy, InfraError, InfraResult};

/// Maximum bytes copied by one canonical file-transfer mutation.
pub const MAX_FILE_TRANSFER_BYTES: u64 = 16 * 1024 * 1024;

/// Explicit source and destination roots for one host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTransferPolicy {
    source: FileReadPolicy,
    destination: FileReadPolicy,
}

impl FileTransferPolicy {
    /// Creates a policy with independent source and destination roots.
    pub fn new<SI, SP, DI, DP>(source_roots: SI, destination_roots: DI) -> InfraResult<Self>
    where
        SI: IntoIterator<Item = SP>,
        SP: Into<PathBuf>,
        DI: IntoIterator<Item = DP>,
        DP: Into<PathBuf>,
    {
        Ok(Self {
            source: FileReadPolicy::new(source_roots)?,
            destination: FileReadPolicy::new(destination_roots)?,
        })
    }

    #[cfg(any(feature = "process-driver", test))]
    pub(crate) fn resolve_source(&self, path: &Path) -> InfraResult<(PathBuf, PathBuf)> {
        ensure_named_file(self.source.resolve(path)?)
    }

    #[cfg(any(feature = "process-driver", test))]
    pub(crate) fn resolve_destination(&self, path: &Path) -> InfraResult<(PathBuf, PathBuf)> {
        ensure_named_file(self.destination.resolve(path)?)
    }
}

/// Stable file content identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferFileIdentity {
    /// Absolute path.
    pub path: PathBuf,
    /// File size.
    pub bytes: u64,
    /// Lowercase SHA-256.
    pub sha256: String,
}

/// Complete authorization-relevant transfer fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTransferFingerprint {
    /// Source host.
    pub source_host: HostId,
    /// Source host revision.
    pub source_revision: TopologyRevision,
    /// Source file identity.
    pub source: TransferFileIdentity,
    /// Destination host.
    pub destination_host: HostId,
    /// Destination host revision.
    pub destination_revision: TopologyRevision,
    /// Destination absolute path.
    pub destination_path: PathBuf,
    /// Existing destination identity, when present.
    pub destination_before: Option<TransferFileIdentity>,
}

/// Deadline-bound transfer request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedFileTransferRequest {
    /// Operation identity.
    pub operation_id: OperationId,
    /// Canonical operation.
    pub operation: OperationName,
    /// Planned transfer fingerprint.
    pub fingerprint: FileTransferFingerprint,
    /// Absolute execution deadline.
    pub deadline: Timestamp,
}

/// Verified file-transfer result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedFileTransferOutcome {
    /// Planned fingerprint.
    pub before: FileTransferFingerprint,
    /// Destination identity after transfer.
    pub destination_after: TransferFileIdentity,
    /// Bytes copied.
    pub bytes: u64,
    /// Backend send state.
    pub send_state: MutationSendState,
    /// Whether source and destination digests match.
    pub verified: bool,
    /// Whether destination content changed.
    pub changed: bool,
}

/// Policy role used while inspecting a transfer path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileTransferPathRole {
    /// Source read policy.
    Source,
    /// Destination write policy.
    Destination,
}

/// Reads file identities for transfer planning and verification.
#[async_trait]
pub trait FileTransferInspector: Send + Sync {
    /// Reads one file identity, optionally returning absence.
    async fn inspect_transfer_file(
        &self,
        host: &HostRecord,
        path: &Path,
        role: FileTransferPathRole,
        optional: bool,
        cancellation: &CancellationToken,
    ) -> InfraResult<Option<TransferFileIdentity>>;
}

/// Complete transfer client used by the verified engine.
pub trait VerifiedFileTransferClient: FileTransfer + FileTransferInspector {}
impl<T> VerifiedFileTransferClient for T where T: FileTransfer + FileTransferInspector {}

#[cfg(any(feature = "process-driver", test))]
fn ensure_named_file((root, relative): (PathBuf, PathBuf)) -> InfraResult<(PathBuf, PathBuf)> {
    if relative.as_os_str().is_empty() {
        Err(InfraError::InvalidRequest {
            domain: "file-transfer",
            message: "transfer path must name a file beneath its configured root".into(),
        })
    } else {
        Ok((root, relative))
    }
}

#[cfg(any(feature = "process-driver", test))]
pub(crate) fn identity_from_bytes(path: &Path, bytes: &[u8]) -> TransferFileIdentity {
    TransferFileIdentity {
        path: path.to_path_buf(),
        bytes: bytes.len() as u64,
        sha256: crate::mutation::sha256_hex(bytes),
    }
}

pub(crate) fn receipt_identity(receipt: &TransferReceipt) -> InfraResult<(&str, &str)> {
    let source = receipt
        .source_sha256()
        .ok_or_else(|| InfraError::InvalidRequest {
            domain: "file-transfer",
            message: "transfer receipt is missing source digest".into(),
        })?;
    let destination = receipt
        .destination_sha256()
        .ok_or_else(|| InfraError::InvalidRequest {
            domain: "file-transfer",
            message: "transfer receipt is missing destination digest".into(),
        })?;
    Ok((source, destination))
}

#[cfg(test)]
#[path = "file_transfer_tests.rs"]
mod tests;
