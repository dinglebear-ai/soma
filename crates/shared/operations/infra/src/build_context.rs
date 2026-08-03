use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::Timestamp;
use tokio_util::sync::CancellationToken;

use crate::{FileReadPolicy, InfraError, InfraResult};

const MAX_CONTEXT_FILES: u32 = 100_000;
const MAX_CONTEXT_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Explicit roots and traversal ceilings for Docker build contexts.
#[derive(Debug, Clone)]
pub struct BuildContextPolicy {
    roots: FileReadPolicy,
    max_files: u32,
    max_bytes: u64,
}

impl BuildContextPolicy {
    /// Creates a build policy from absolute admitted roots.
    pub fn new<I, P>(roots: I) -> InfraResult<Self>
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        Ok(Self {
            roots: FileReadPolicy::new(roots)?,
            max_files: 25_000,
            max_bytes: 2 * 1024 * 1024 * 1024,
        })
    }

    /// Sets bounded context file and byte ceilings.
    pub fn with_limits(mut self, max_files: u32, max_bytes: u64) -> InfraResult<Self> {
        if max_files == 0 || max_files > MAX_CONTEXT_FILES {
            return Err(InfraError::InvalidRequest {
                domain: "build-context",
                message: format!("file limit must be 1-{MAX_CONTEXT_FILES}"),
            });
        }
        if max_bytes == 0 || max_bytes > MAX_CONTEXT_BYTES {
            return Err(InfraError::InvalidRequest {
                domain: "build-context",
                message: format!("byte limit must be 1-{MAX_CONTEXT_BYTES}"),
            });
        }
        self.max_files = max_files;
        self.max_bytes = max_bytes;
        Ok(self)
    }

    /// Returns admitted roots.
    pub fn roots(&self) -> impl Iterator<Item = &Path> {
        self.roots.roots()
    }

    /// Returns the file ceiling.
    #[must_use]
    pub const fn max_files(&self) -> u32 {
        self.max_files
    }

    /// Returns the byte ceiling.
    #[must_use]
    pub const fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    #[cfg(any(feature = "process-driver", test))]
    pub(crate) fn resolve(&self, path: &Path) -> InfraResult<(PathBuf, PathBuf)> {
        self.roots.resolve(path)
    }
}

/// Deterministic content fingerprint for one admitted build context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildContextFingerprint {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Absolute build context path.
    pub path: PathBuf,
    /// Lowercase SHA-256 over relative paths, modes, sizes, and regular-file content.
    pub sha256: String,
    /// Number of regular files hashed.
    pub file_count: u32,
    /// Total regular-file bytes hashed.
    pub byte_count: u64,
}

impl BuildContextFingerprint {
    /// Validates the fingerprint wire representation.
    pub fn validate(&self) -> InfraResult<()> {
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(InfraError::Parse {
                domain: "build-context",
                message: "context fingerprint is not lowercase SHA-256".into(),
            });
        }
        Ok(())
    }
}

/// Reads one build context through descriptor-confined traversal.
#[async_trait]
pub trait BuildContextInspector: Send + Sync {
    /// Computes one bounded deterministic context fingerprint.
    async fn fingerprint(
        &self,
        host: &HostRecord,
        path: &Path,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<BuildContextFingerprint>;
}

#[cfg(test)]
#[path = "build_context_tests.rs"]
mod tests;
