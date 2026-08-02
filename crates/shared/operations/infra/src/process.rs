use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::Timestamp;
use tokio_util::sync::CancellationToken;

use crate::{InfraError, InfraResult};

const MAX_FILTER_CHARS: usize = 1024;
const MAX_PROCESS_ROWS: u32 = 500;

/// Supported deterministic process sort orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProcessSort {
    /// Highest CPU utilization first.
    #[default]
    Cpu,
    /// Highest memory utilization first.
    Memory,
    /// Lowest process identifier first.
    Pid,
    /// Greatest accumulated CPU time first.
    Time,
}

impl ProcessSort {
    #[cfg(any(feature = "process-driver", test))]
    pub(crate) const fn ps_argument(self) -> &'static str {
        match self {
            Self::Cpu => "-cpu",
            Self::Memory => "-mem",
            Self::Pid => "pid",
            Self::Time => "-time",
        }
    }
}

/// Closed request for one process snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessListRequest {
    sort: ProcessSort,
    grep: Option<String>,
    user: Option<String>,
    limit: u32,
    deadline: Timestamp,
}

impl ProcessListRequest {
    /// Creates a CPU-sorted request limited to 50 rows.
    #[must_use]
    pub const fn new(deadline: Timestamp) -> Self {
        Self {
            sort: ProcessSort::Cpu,
            grep: None,
            user: None,
            limit: 50,
            deadline,
        }
    }

    /// Selects the process sort order.
    #[must_use]
    pub const fn with_sort(mut self, sort: ProcessSort) -> Self {
        self.sort = sort;
        self
    }

    /// Adds a case-sensitive substring filter over rendered command rows.
    pub fn with_grep(mut self, value: impl Into<String>) -> InfraResult<Self> {
        self.grep = Some(validate_filter("grep", value.into())?);
        Ok(self)
    }

    /// Adds an exact user-column filter.
    pub fn with_user(mut self, value: impl Into<String>) -> InfraResult<Self> {
        self.user = Some(validate_filter("user", value.into())?);
        Ok(self)
    }

    /// Sets the maximum returned row count.
    pub fn with_limit(mut self, limit: u32) -> InfraResult<Self> {
        if limit == 0 || limit > MAX_PROCESS_ROWS {
            return Err(InfraError::InvalidRequest {
                domain: "process",
                message: format!("limit must be 1-{MAX_PROCESS_ROWS}"),
            });
        }
        self.limit = limit;
        Ok(self)
    }

    /// Returns the selected sort order.
    #[must_use]
    pub const fn sort(&self) -> ProcessSort {
        self.sort
    }

    /// Returns the optional command substring filter.
    #[must_use]
    pub fn grep(&self) -> Option<&str> {
        self.grep.as_deref()
    }

    /// Returns the optional user filter.
    #[must_use]
    pub fn user(&self) -> Option<&str> {
        self.user.as_deref()
    }

    /// Returns the row limit.
    #[must_use]
    pub const fn limit(&self) -> u32 {
        self.limit
    }

    /// Returns the absolute request deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Typed row from a process snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessRow {
    /// Process owner.
    pub user: String,
    /// Process identifier.
    pub pid: u32,
    /// CPU percentage reported by ps.
    pub cpu_percent: f64,
    /// Memory percentage reported by ps.
    pub memory_percent: f64,
    /// Virtual memory size in KiB.
    pub virtual_size_kib: u64,
    /// Resident memory size in KiB.
    pub resident_size_kib: u64,
    /// Controlling terminal.
    pub tty: String,
    /// Process state flags.
    pub state: String,
    /// Process start field reported by ps.
    pub start: String,
    /// Accumulated CPU time.
    pub cpu_time: String,
    /// Command and arguments.
    pub command: String,
}

/// Bounded process snapshot for one host revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessSnapshot {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Applied sort order.
    pub sort: ProcessSort,
    /// Returned process rows.
    pub rows: Vec<ProcessRow>,
    /// Whether rows were omitted by the request limit.
    pub truncated: bool,
}

/// Product-neutral process inspection engine.
#[async_trait]
pub trait ProcessInspector: Send + Sync {
    /// Lists and parses a bounded process snapshot.
    async fn list_processes(
        &self,
        host: &HostRecord,
        request: &ProcessListRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<ProcessSnapshot>;
}

#[cfg(any(feature = "process-driver", test))]
pub(crate) fn parse_process_rows(
    host: &HostRecord,
    request: &ProcessListRequest,
    raw: &str,
) -> InfraResult<ProcessSnapshot> {
    let mut rows = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(parse_process_row)
        .collect::<InfraResult<Vec<_>>>()?;

    if let Some(user) = request.user() {
        rows.retain(|row| row.user == user);
    }
    if let Some(pattern) = request.grep() {
        rows.retain(|row| row.command.contains(pattern));
    }

    let truncated = rows.len() > request.limit() as usize;
    rows.truncate(request.limit() as usize);
    Ok(ProcessSnapshot {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        sort: request.sort(),
        rows,
        truncated,
    })
}

#[cfg(any(feature = "process-driver", test))]
fn parse_process_row(line: &str) -> InfraResult<ProcessRow> {
    let mut fields = line.split_whitespace();
    let user = next_field(&mut fields, "user")?.to_owned();
    let pid = parse_field(next_field(&mut fields, "pid")?, "pid")?;
    let cpu_percent = parse_field(next_field(&mut fields, "cpu")?, "cpu")?;
    let memory_percent = parse_field(next_field(&mut fields, "memory")?, "memory")?;
    let virtual_size_kib = parse_field(next_field(&mut fields, "vsz")?, "vsz")?;
    let resident_size_kib = parse_field(next_field(&mut fields, "rss")?, "rss")?;
    let tty = next_field(&mut fields, "tty")?.to_owned();
    let state = next_field(&mut fields, "state")?.to_owned();
    let start = next_field(&mut fields, "start")?.to_owned();
    let cpu_time = next_field(&mut fields, "time")?.to_owned();
    let command = fields.collect::<Vec<_>>().join(" ");
    if command.is_empty() {
        return Err(parse_error("process row has no command"));
    }
    Ok(ProcessRow {
        user,
        pid,
        cpu_percent,
        memory_percent,
        virtual_size_kib,
        resident_size_kib,
        tty,
        state,
        start,
        cpu_time,
        command,
    })
}

#[cfg(any(feature = "process-driver", test))]
fn next_field<'a>(fields: &mut impl Iterator<Item = &'a str>, name: &str) -> InfraResult<&'a str> {
    fields
        .next()
        .ok_or_else(|| parse_error(&format!("process row has no {name} field")))
}

#[cfg(any(feature = "process-driver", test))]
fn parse_field<T>(value: &str, name: &str) -> InfraResult<T>
where
    T: std::str::FromStr,
{
    value
        .parse()
        .map_err(|_| parse_error(&format!("invalid process {name} field: {value:?}")))
}

fn validate_filter(name: &'static str, value: String) -> InfraResult<String> {
    let count = value.chars().count();
    if count == 0 || count > MAX_FILTER_CHARS || value.chars().any(char::is_control) {
        Err(InfraError::InvalidRequest {
            domain: "process",
            message: format!("{name} must contain 1-{MAX_FILTER_CHARS} printable characters"),
        })
    } else {
        Ok(value)
    }
}

#[cfg(any(feature = "process-driver", test))]
fn parse_error(message: &str) -> InfraError {
    InfraError::Parse {
        domain: "process",
        message: message.to_owned(),
    }
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
