use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use tokio_util::sync::CancellationToken;

use crate::{InfraError, InfraResult};

const MAX_LOG_LINES: u32 = 5000;
const MAX_GREP_CHARS: usize = 1024;

/// Aggregate disk usage for one Docker resource category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DockerUsageCategory {
    /// Number of reported resources.
    pub count: u64,
    /// Sum of resource size fields in bytes.
    pub size_bytes: u64,
}

/// Neutral Docker disk-usage snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockerDiskUsage {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Shared layer bytes reported by the daemon.
    pub layers_size_bytes: u64,
    /// Image usage.
    pub images: DockerUsageCategory,
    /// Container writable-layer usage.
    pub containers: DockerUsageCategory,
    /// Local volume usage.
    pub volumes: DockerUsageCategory,
    /// Build-cache usage.
    pub build_cache: DockerUsageCategory,
}

/// Selected Docker log stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DockerLogStream {
    /// Standard output only.
    Stdout,
    /// Standard error only.
    Stderr,
    /// Both output streams.
    #[default]
    Both,
}

/// Bounded one-shot Docker log options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerLogOptions {
    lines: u32,
    stream: DockerLogStream,
    since_unix_seconds: Option<i64>,
    until_unix_seconds: Option<i64>,
    grep: Option<String>,
}

impl Default for ContainerLogOptions {
    fn default() -> Self {
        Self {
            lines: 100,
            stream: DockerLogStream::Both,
            since_unix_seconds: None,
            until_unix_seconds: None,
            grep: None,
        }
    }
}

impl ContainerLogOptions {
    /// Sets the requested tail line count.
    pub fn with_lines(mut self, lines: u32) -> InfraResult<Self> {
        if lines == 0 || lines > MAX_LOG_LINES {
            return Err(InfraError::InvalidRequest {
                domain: "docker",
                message: format!("container log lines must be 1-{MAX_LOG_LINES}"),
            });
        }
        self.lines = lines;
        Ok(self)
    }

    /// Selects the output stream.
    #[must_use]
    pub const fn with_stream(mut self, stream: DockerLogStream) -> Self {
        self.stream = stream;
        self
    }

    /// Sets an inclusive lower Unix-second bound.
    pub fn with_since(mut self, seconds: i64) -> InfraResult<Self> {
        self.since_unix_seconds = Some(seconds);
        self.validate_time_order()?;
        Ok(self)
    }

    /// Sets an inclusive upper Unix-second bound.
    pub fn with_until(mut self, seconds: i64) -> InfraResult<Self> {
        self.until_unix_seconds = Some(seconds);
        self.validate_time_order()?;
        Ok(self)
    }

    /// Adds a local case-sensitive substring filter.
    pub fn with_grep(mut self, grep: impl Into<String>) -> InfraResult<Self> {
        let grep = grep.into();
        let count = grep.chars().count();
        if count == 0 || count > MAX_GREP_CHARS || grep.chars().any(char::is_control) {
            return Err(InfraError::InvalidRequest {
                domain: "docker",
                message: format!(
                    "container log grep must be 1-{MAX_GREP_CHARS} printable characters"
                ),
            });
        }
        self.grep = Some(grep);
        Ok(self)
    }

    /// Returns the line count.
    #[must_use]
    pub const fn lines(&self) -> u32 {
        self.lines
    }

    /// Returns the selected stream.
    #[must_use]
    pub const fn stream(&self) -> DockerLogStream {
        self.stream
    }

    /// Returns the lower Unix-second bound.
    #[must_use]
    pub const fn since_unix_seconds(&self) -> Option<i64> {
        self.since_unix_seconds
    }

    /// Returns the upper Unix-second bound.
    #[must_use]
    pub const fn until_unix_seconds(&self) -> Option<i64> {
        self.until_unix_seconds
    }

    /// Returns the optional local grep filter.
    #[must_use]
    pub fn grep(&self) -> Option<&str> {
        self.grep.as_deref()
    }

    fn validate_time_order(&self) -> InfraResult<()> {
        if matches!(
            (self.since_unix_seconds, self.until_unix_seconds),
            (Some(since), Some(until)) if since > until
        ) {
            Err(InfraError::InvalidRequest {
                domain: "docker",
                message: "container log since must not exceed until".into(),
            })
        } else {
            Ok(())
        }
    }
}

/// Bounded one-shot Docker log result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerLogs {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Container identifier.
    pub container: String,
    /// Rendered non-empty log lines.
    pub lines: Vec<String>,
    /// Whether the client-side byte ceiling omitted output.
    pub truncated: bool,
}

/// Neutral one-shot Docker container statistics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerStatsSnapshot {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Container identifier.
    pub container: String,
    /// Daemon read timestamp.
    pub read_at: Option<String>,
    /// Current process count.
    pub pids_current: u64,
    /// Current memory usage.
    pub memory_usage_bytes: u64,
    /// Memory limit.
    pub memory_limit_bytes: u64,
    /// Total container CPU usage.
    pub cpu_total_usage: u64,
    /// Host system CPU usage.
    pub system_cpu_usage: u64,
    /// Online CPU count.
    pub online_cpus: u64,
    /// Aggregate received network bytes.
    pub network_rx_bytes: u64,
    /// Aggregate transmitted network bytes.
    pub network_tx_bytes: u64,
    /// Aggregate block-device read bytes.
    pub block_read_bytes: u64,
    /// Aggregate block-device write bytes.
    pub block_write_bytes: u64,
}

/// Docker telemetry read operations.
#[async_trait]
pub trait DockerTelemetryReader: Send + Sync {
    /// Reads daemon disk usage.
    async fn disk_usage(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> InfraResult<DockerDiskUsage>;

    /// Reads bounded one-shot container logs.
    async fn container_logs(
        &self,
        host: &HostRecord,
        container: &str,
        options: &ContainerLogOptions,
        cancellation: &CancellationToken,
    ) -> InfraResult<ContainerLogs>;

    /// Reads one container statistics frame.
    async fn container_stats(
        &self,
        host: &HostRecord,
        container: &str,
        cancellation: &CancellationToken,
    ) -> InfraResult<ContainerStatsSnapshot>;
}

#[cfg(test)]
#[path = "docker_telemetry_tests.rs"]
mod tests;
