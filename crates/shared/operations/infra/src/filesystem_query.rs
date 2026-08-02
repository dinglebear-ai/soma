use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::Timestamp;
use tokio_util::sync::CancellationToken;

use crate::{FileKind, InfraError, InfraResult};

const MAX_DEPTH: u8 = 20;
const MAX_RESULTS: u32 = 500;
const MAX_TAIL_LINES: u32 = 5000;

/// Request for a bounded file or directory read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathReadRequest {
    tree: bool,
    depth: u8,
    deadline: Timestamp,
}

impl PathReadRequest {
    /// Creates a direct path read.
    #[must_use]
    pub const fn new(deadline: Timestamp) -> Self {
        Self {
            tree: false,
            depth: 3,
            deadline,
        }
    }
    /// Enables a bounded directory tree.
    pub fn with_tree(mut self, depth: u8) -> InfraResult<Self> {
        if depth == 0 || depth > MAX_DEPTH {
            return Err(InfraError::InvalidRequest {
                domain: "filesystem",
                message: format!("tree depth must be 1-{MAX_DEPTH}"),
            });
        }
        self.tree = true;
        self.depth = depth;
        Ok(self)
    }
    /// Returns whether tree mode is enabled.
    #[must_use]
    pub const fn tree(&self) -> bool {
        self.tree
    }
    /// Returns the tree depth.
    #[must_use]
    pub const fn depth(&self) -> u8 {
        self.depth
    }
    /// Returns the deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Bounded file or directory read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathRead {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Requested absolute path.
    pub path: PathBuf,
    /// Object kind.
    pub kind: FileKind,
    /// File bytes when the target is a regular file.
    pub content: Vec<u8>,
    /// Directory entries or tree paths.
    pub entries: Vec<String>,
    /// Original file size.
    pub size_bytes: u64,
    /// Whether content or entries were truncated.
    pub truncated: bool,
}

/// Bounded recursive file search request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileFindRequest {
    pattern: String,
    depth: u8,
    limit: u32,
    deadline: Timestamp,
}

impl FileFindRequest {
    /// Creates a search request.
    pub fn new(pattern: impl Into<String>, deadline: Timestamp) -> InfraResult<Self> {
        let pattern = pattern.into();
        validate_pattern(&pattern)?;
        Ok(Self {
            pattern,
            depth: 10,
            limit: MAX_RESULTS,
            deadline,
        })
    }
    /// Sets traversal depth.
    pub fn with_depth(mut self, depth: u8) -> InfraResult<Self> {
        if depth == 0 || depth > MAX_DEPTH {
            return Err(InfraError::InvalidRequest {
                domain: "filesystem",
                message: format!("find depth must be 1-{MAX_DEPTH}"),
            });
        }
        self.depth = depth;
        Ok(self)
    }
    /// Sets result limit.
    pub fn with_limit(mut self, limit: u32) -> InfraResult<Self> {
        if limit == 0 || limit > MAX_RESULTS {
            return Err(InfraError::InvalidRequest {
                domain: "filesystem",
                message: format!("find limit must be 1-{MAX_RESULTS}"),
            });
        }
        self.limit = limit;
        Ok(self)
    }
    /// Returns the glob pattern.
    #[must_use]
    pub fn pattern(&self) -> &str {
        &self.pattern
    }
    /// Returns traversal depth.
    #[must_use]
    pub const fn depth(&self) -> u8 {
        self.depth
    }
    /// Returns result limit.
    #[must_use]
    pub const fn limit(&self) -> u32 {
        self.limit
    }
    /// Returns deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Bounded file-search result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSearch {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Search root.
    pub path: PathBuf,
    /// Matching absolute paths.
    pub items: Vec<PathBuf>,
    /// Whether the result or visit ceiling was reached.
    pub truncated: bool,
}

/// Request for a bounded file tail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTailRequest {
    lines: u32,
    deadline: Timestamp,
}

impl FileTailRequest {
    /// Creates a request for the last 100 lines.
    #[must_use]
    pub const fn new(deadline: Timestamp) -> Self {
        Self {
            lines: 100,
            deadline,
        }
    }
    /// Sets line count.
    pub fn with_lines(mut self, lines: u32) -> InfraResult<Self> {
        if lines == 0 || lines > MAX_TAIL_LINES {
            return Err(InfraError::InvalidRequest {
                domain: "filesystem",
                message: format!("tail lines must be 1-{MAX_TAIL_LINES}"),
            });
        }
        self.lines = lines;
        Ok(self)
    }
    /// Returns line count.
    #[must_use]
    pub const fn lines(&self) -> u32 {
        self.lines
    }
    /// Returns deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Bounded file tail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTail {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Requested path.
    pub path: PathBuf,
    /// Retained UTF-8 bytes.
    pub content: Vec<u8>,
    /// Returned line count.
    pub line_count: usize,
    /// Whether the byte window omitted earlier content.
    pub truncated: bool,
}

/// Descriptor-confined filesystem queries usable locally or over SSH.
#[async_trait]
pub trait FilesystemQueryInspector: Send + Sync {
    /// Reads a file, directory, or bounded tree.
    async fn read_path(
        &self,
        host: &HostRecord,
        path: &Path,
        request: &PathReadRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<PathRead>;
    /// Finds files recursively beneath one admitted root.
    async fn find(
        &self,
        host: &HostRecord,
        path: &Path,
        request: &FileFindRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<FileSearch>;
    /// Returns the last lines of one admitted regular file.
    async fn tail(
        &self,
        host: &HostRecord,
        path: &Path,
        request: &FileTailRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<FileTail>;
}

fn validate_pattern(value: &str) -> InfraResult<()> {
    if value.is_empty()
        || value.starts_with('-')
        || value.chars().count() > 256
        || value.chars().any(char::is_control)
    {
        Err(InfraError::InvalidRequest {
            domain: "filesystem",
            message: "invalid find pattern".into(),
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "filesystem_query_tests.rs"]
mod tests;
