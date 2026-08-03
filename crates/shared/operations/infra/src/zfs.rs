use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::Timestamp;
use tokio_util::sync::CancellationToken;

use crate::{InfraError, InfraResult};

const MAX_TARGET_CHARS: usize = 256;
const MAX_ROWS: u32 = 5000;

/// Allowlisted ZFS dataset types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZfsDatasetType {
    /// Filesystems.
    Filesystem,
    /// Block volumes.
    Volume,
    /// Snapshots.
    Snapshot,
    /// Bookmarks.
    Bookmark,
    /// Every supported type.
    All,
}

impl ZfsDatasetType {
    #[cfg(any(feature = "process-driver", test))]
    pub(crate) const fn as_arg(self) -> &'static str {
        match self {
            Self::Filesystem => "filesystem",
            Self::Volume => "volume",
            Self::Snapshot => "snapshot",
            Self::Bookmark => "bookmark",
            Self::All => "all",
        }
    }
}

/// Request for a ZFS pool listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZfsPoolRequest {
    pool: Option<String>,
    deadline: Timestamp,
}

impl ZfsPoolRequest {
    /// Creates an unfiltered pool request.
    #[must_use]
    pub const fn new(deadline: Timestamp) -> Self {
        Self {
            pool: None,
            deadline,
        }
    }

    /// Restricts the listing to one pool.
    pub fn with_pool(mut self, pool: impl Into<String>) -> InfraResult<Self> {
        self.pool = Some(validate_target("pool", pool.into())?);
        Ok(self)
    }

    /// Returns the optional pool filter.
    #[must_use]
    pub fn pool(&self) -> Option<&str> {
        self.pool.as_deref()
    }

    /// Returns the absolute deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Request for a ZFS dataset listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZfsDatasetRequest {
    pool: Option<String>,
    dataset_type: Option<ZfsDatasetType>,
    recursive: bool,
    deadline: Timestamp,
}

impl ZfsDatasetRequest {
    /// Creates an unfiltered dataset request.
    #[must_use]
    pub const fn new(deadline: Timestamp) -> Self {
        Self {
            pool: None,
            dataset_type: None,
            recursive: false,
            deadline,
        }
    }

    /// Restricts the listing to one pool or dataset root.
    pub fn with_pool(mut self, pool: impl Into<String>) -> InfraResult<Self> {
        self.pool = Some(validate_target("pool", pool.into())?);
        Ok(self)
    }

    /// Selects a dataset type.
    #[must_use]
    pub const fn with_type(mut self, dataset_type: ZfsDatasetType) -> Self {
        self.dataset_type = Some(dataset_type);
        self
    }

    /// Enables recursive listing.
    #[must_use]
    pub const fn recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    /// Returns the optional pool filter.
    #[must_use]
    pub fn pool(&self) -> Option<&str> {
        self.pool.as_deref()
    }

    /// Returns the optional dataset type.
    #[must_use]
    pub const fn dataset_type(&self) -> Option<ZfsDatasetType> {
        self.dataset_type
    }

    /// Returns whether recursive listing is enabled.
    #[must_use]
    pub const fn is_recursive(&self) -> bool {
        self.recursive
    }

    /// Returns the absolute deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Request for a bounded ZFS snapshot listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZfsSnapshotRequest {
    pool: Option<String>,
    dataset: Option<String>,
    limit: u32,
    deadline: Timestamp,
}

impl ZfsSnapshotRequest {
    /// Creates an unfiltered request limited to 500 rows.
    #[must_use]
    pub const fn new(deadline: Timestamp) -> Self {
        Self {
            pool: None,
            dataset: None,
            limit: 500,
            deadline,
        }
    }

    /// Sets a pool fallback target.
    pub fn with_pool(mut self, pool: impl Into<String>) -> InfraResult<Self> {
        self.pool = Some(validate_target("pool", pool.into())?);
        Ok(self)
    }

    /// Sets a dataset target, which takes precedence over the pool.
    pub fn with_dataset(mut self, dataset: impl Into<String>) -> InfraResult<Self> {
        self.dataset = Some(validate_target("dataset", dataset.into())?);
        Ok(self)
    }

    /// Sets the maximum returned rows.
    pub fn with_limit(mut self, limit: u32) -> InfraResult<Self> {
        if limit == 0 || limit > MAX_ROWS {
            return Err(InfraError::InvalidRequest {
                domain: "zfs",
                message: format!("snapshot limit must be 1-{MAX_ROWS}"),
            });
        }
        self.limit = limit;
        Ok(self)
    }

    /// Returns the pool fallback.
    #[must_use]
    pub fn pool(&self) -> Option<&str> {
        self.pool.as_deref()
    }

    /// Returns the dataset target.
    #[must_use]
    pub fn dataset(&self) -> Option<&str> {
        self.dataset.as_deref()
    }

    /// Returns the row limit.
    #[must_use]
    pub const fn limit(&self) -> u32 {
        self.limit
    }

    /// Returns the absolute deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Structured ZFS tabular output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZfsTable {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Column names in source order.
    pub columns: Vec<String>,
    /// Rows keyed by column name.
    pub rows: Vec<BTreeMap<String, String>>,
    /// Whether rows were omitted by the request limit.
    pub truncated: bool,
}

/// Product-neutral ZFS read engine.
#[async_trait]
pub trait ZfsInspector: Send + Sync {
    /// Lists pools.
    async fn pools(
        &self,
        host: &HostRecord,
        request: &ZfsPoolRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<ZfsTable>;

    /// Lists datasets.
    async fn datasets(
        &self,
        host: &HostRecord,
        request: &ZfsDatasetRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<ZfsTable>;

    /// Lists snapshots.
    async fn snapshots(
        &self,
        host: &HostRecord,
        request: &ZfsSnapshotRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<ZfsTable>;
}

#[cfg(any(feature = "process-driver", test))]
pub(crate) fn parse_zfs_table(
    host: &HostRecord,
    raw: &str,
    limit: Option<u32>,
) -> InfraResult<ZfsTable> {
    let mut lines = raw.lines().filter(|line| !line.trim().is_empty());
    let columns = lines
        .next()
        .ok_or_else(|| parse_error("ZFS output has no header"))?
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if columns.is_empty() {
        return Err(parse_error("ZFS output has an empty header"));
    }
    let mut rows = lines
        .map(|line| parse_row(&columns, line))
        .collect::<InfraResult<Vec<_>>>()?;
    let truncated = limit.is_some_and(|limit| rows.len() > limit as usize);
    if let Some(limit) = limit {
        rows.truncate(limit as usize);
    }
    Ok(ZfsTable {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        columns,
        rows,
        truncated,
    })
}

#[cfg(any(feature = "process-driver", test))]
fn parse_row(columns: &[String], line: &str) -> InfraResult<BTreeMap<String, String>> {
    let values = line.split_whitespace().collect::<Vec<_>>();
    if values.len() < columns.len() {
        return Err(parse_error(&format!(
            "ZFS row has {} values for {} columns",
            values.len(),
            columns.len()
        )));
    }
    let mut row = BTreeMap::new();
    for (index, column) in columns.iter().enumerate() {
        let value = if index + 1 == columns.len() {
            values[index..].join(" ")
        } else {
            values[index].to_owned()
        };
        row.insert(column.clone(), value);
    }
    Ok(row)
}

fn validate_target(kind: &'static str, value: String) -> InfraResult<String> {
    let count = value.chars().count();
    if count == 0
        || count > MAX_TARGET_CHARS
        || value.starts_with('-')
        || value.chars().any(|character| {
            !(character.is_ascii_alphanumeric()
                || matches!(character, '_' | '-' | '.' | ':' | '/' | '@'))
        })
    {
        Err(InfraError::InvalidRequest {
            domain: "zfs",
            message: format!("invalid {kind} target: {value:?}"),
        })
    } else {
        Ok(value)
    }
}

#[cfg(any(feature = "process-driver", test))]
fn parse_error(message: &str) -> InfraError {
    InfraError::Parse {
        domain: "zfs",
        message: message.to_owned(),
    }
}

#[cfg(test)]
#[path = "zfs_tests.rs"]
mod tests;
