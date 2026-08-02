use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use tokio_util::sync::CancellationToken;

use crate::{InfraError, InfraResult};

const MAX_PREVIEW_BYTES: usize = 16 * 1024 * 1024;
const MAX_HASH_BYTES: u64 = 1024 * 1024 * 1024 * 1024;

/// Closed read policy for one filesystem inspector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReadPolicy {
    roots: Vec<PathBuf>,
    max_preview_bytes: usize,
    max_hash_bytes: u64,
}

impl FileReadPolicy {
    /// Creates a read policy with absolute normalized roots.
    pub fn new<I, P>(roots: I) -> InfraResult<Self>
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        let mut roots = roots
            .into_iter()
            .map(Into::into)
            .map(validate_absolute_path)
            .collect::<InfraResult<Vec<_>>>()?;
        roots.sort();
        roots.dedup();
        if roots.is_empty() {
            return Err(InfraError::InvalidRequest {
                domain: "filesystem",
                message: "at least one read root is required".into(),
            });
        }
        Ok(Self {
            roots,
            max_preview_bytes: 1024 * 1024,
            max_hash_bytes: 1024 * 1024 * 1024,
        })
    }

    /// Sets the maximum preview bytes retained in memory.
    pub fn with_preview_limit(mut self, bytes: usize) -> InfraResult<Self> {
        if bytes == 0 || bytes > MAX_PREVIEW_BYTES {
            return Err(InfraError::InvalidRequest {
                domain: "filesystem",
                message: format!("preview limit must be 1-{MAX_PREVIEW_BYTES} bytes"),
            });
        }
        self.max_preview_bytes = bytes;
        Ok(self)
    }

    /// Sets the maximum file size admitted for hashing.
    pub fn with_hash_limit(mut self, bytes: u64) -> InfraResult<Self> {
        if bytes == 0 || bytes > MAX_HASH_BYTES {
            return Err(InfraError::InvalidRequest {
                domain: "filesystem",
                message: format!("hash limit must be 1-{MAX_HASH_BYTES} bytes"),
            });
        }
        self.max_hash_bytes = bytes;
        Ok(self)
    }

    /// Returns admitted roots in deterministic order.
    pub fn roots(&self) -> impl Iterator<Item = &Path> {
        self.roots.iter().map(PathBuf::as_path)
    }

    /// Returns the preview byte limit.
    #[must_use]
    pub const fn max_preview_bytes(&self) -> usize {
        self.max_preview_bytes
    }

    /// Returns the hash byte limit.
    #[must_use]
    pub const fn max_hash_bytes(&self) -> u64 {
        self.max_hash_bytes
    }

    #[cfg(any(feature = "linux-filesystem", test))]
    pub(crate) fn resolve(&self, path: &Path) -> InfraResult<(PathBuf, PathBuf)> {
        let path = validate_absolute_path(path.to_path_buf())?;
        let root = self
            .roots
            .iter()
            .filter(|root| {
                path == **root || root.as_os_str() == "/" || path.strip_prefix(root).is_ok()
            })
            .max_by_key(|root| root.components().count())
            .cloned()
            .ok_or_else(|| InfraError::PathOutsideRoots(path.clone()))?;
        let relative = path
            .strip_prefix(&root)
            .map_err(|_| InfraError::PathOutsideRoots(path.clone()))?
            .to_path_buf();
        Ok((root, relative))
    }
}

/// Read-only filesystem object kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    /// Regular file.
    File,
    /// Directory.
    Directory,
}

/// Typed filesystem metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMetadata {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Requested absolute path.
    pub path: PathBuf,
    /// Object kind.
    pub kind: FileKind,
    /// File length in bytes, or zero for directories.
    pub size_bytes: u64,
    /// Whether the current metadata marks the object read-only.
    pub readonly: bool,
    /// Last-modified time in Unix milliseconds when available.
    pub modified_unix_millis: Option<i64>,
}

/// Bounded file preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilePreview {
    /// File metadata.
    pub metadata: FileMetadata,
    /// Retained content prefix.
    pub content: Vec<u8>,
    /// Whether the file exceeded the preview limit.
    pub truncated: bool,
}

/// SHA-256 file digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileHash {
    /// File metadata.
    pub metadata: FileMetadata,
    /// Lowercase SHA-256 digest.
    pub sha256: String,
    /// Number of bytes hashed.
    pub bytes_hashed: u64,
}

/// Product-neutral filesystem inspection engine.
#[async_trait]
pub trait FilesystemInspector: Send + Sync {
    /// Returns metadata for one admitted path.
    async fn stat(
        &self,
        host: &HostRecord,
        path: &Path,
        cancellation: &CancellationToken,
    ) -> InfraResult<FileMetadata>;

    /// Reads a bounded prefix of one admitted regular file.
    async fn read(
        &self,
        host: &HostRecord,
        path: &Path,
        cancellation: &CancellationToken,
    ) -> InfraResult<FilePreview>;

    /// Hashes one admitted regular file within the policy size limit.
    async fn hash(
        &self,
        host: &HostRecord,
        path: &Path,
        cancellation: &CancellationToken,
    ) -> InfraResult<FileHash>;
}

fn validate_absolute_path(path: PathBuf) -> InfraResult<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        Err(InfraError::InvalidRequest {
            domain: "filesystem",
            message: format!("path must be absolute and normalized: {}", path.display()),
        })
    } else {
        Ok(path)
    }
}

#[cfg(test)]
#[path = "filesystem_tests.rs"]
mod tests;
