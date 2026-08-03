use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::Timestamp;
use tokio_util::sync::CancellationToken;

use crate::{InfraError, InfraResult};

const MAX_LINES: u32 = 500;
const MAX_FILTER_CHARS: usize = 1024;
const MAX_UNIT_CHARS: usize = 256;
const MAX_TIME_CHARS: usize = 64;

/// Supported read-only operating-system log sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogSource {
    /// Traditional system log, with messages fallback.
    Syslog,
    /// systemd journal.
    Journal,
    /// Kernel ring buffer.
    Dmesg,
    /// Authentication log, with secure fallback.
    Auth,
}

/// Journal priority accepted by journalctl.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalPriority {
    /// Emergency.
    Emerg,
    /// Alert.
    Alert,
    /// Critical.
    Crit,
    /// Error.
    Err,
    /// Warning.
    Warning,
    /// Notice.
    Notice,
    /// Informational.
    Info,
    /// Debug.
    Debug,
}

impl JournalPriority {
    #[cfg(any(feature = "process-driver", test))]
    pub(crate) const fn as_arg(self) -> &'static str {
        match self {
            Self::Emerg => "emerg",
            Self::Alert => "alert",
            Self::Crit => "crit",
            Self::Err => "err",
            Self::Warning => "warning",
            Self::Notice => "notice",
            Self::Info => "info",
            Self::Debug => "debug",
        }
    }
}

/// Validated journal filters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JournalFilters {
    unit: Option<String>,
    priority: Option<JournalPriority>,
    since: Option<String>,
    until: Option<String>,
}

impl JournalFilters {
    /// Adds a journal unit filter.
    pub fn with_unit(mut self, unit: impl Into<String>) -> InfraResult<Self> {
        let unit = unit.into();
        if unit.is_empty()
            || unit.starts_with('-')
            || unit.chars().count() > MAX_UNIT_CHARS
            || unit.chars().any(char::is_control)
        {
            return Err(InfraError::InvalidRequest {
                domain: "logs",
                message: "journal unit must be 1-256 printable characters and not start with '-'"
                    .into(),
            });
        }
        self.unit = Some(unit);
        Ok(self)
    }

    /// Adds a journal priority filter.
    #[must_use]
    pub const fn with_priority(mut self, priority: JournalPriority) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Adds a journal lower time bound.
    pub fn with_since(mut self, since: impl Into<String>) -> InfraResult<Self> {
        self.since = Some(validate_time_filter(since.into())?);
        Ok(self)
    }

    /// Adds a journal upper time bound.
    pub fn with_until(mut self, until: impl Into<String>) -> InfraResult<Self> {
        self.until = Some(validate_time_filter(until.into())?);
        Ok(self)
    }

    /// Returns the optional unit filter.
    #[must_use]
    pub fn unit(&self) -> Option<&str> {
        self.unit.as_deref()
    }

    /// Returns the optional priority.
    #[must_use]
    pub const fn priority(&self) -> Option<JournalPriority> {
        self.priority
    }

    /// Returns the optional lower time bound.
    #[must_use]
    pub fn since(&self) -> Option<&str> {
        self.since.as_deref()
    }

    /// Returns the optional upper time bound.
    #[must_use]
    pub fn until(&self) -> Option<&str> {
        self.until.as_deref()
    }
}

/// Bounded read request for one log source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogReadRequest {
    source: LogSource,
    lines: u32,
    grep: Option<String>,
    journal: JournalFilters,
    deadline: Timestamp,
}

impl LogReadRequest {
    /// Creates a request for 100 lines from the selected source.
    #[must_use]
    pub const fn new(source: LogSource, deadline: Timestamp) -> Self {
        Self {
            source,
            lines: 100,
            grep: None,
            journal: JournalFilters {
                unit: None,
                priority: None,
                since: None,
                until: None,
            },
            deadline,
        }
    }

    /// Sets the maximum returned line count.
    pub fn with_lines(mut self, lines: u32) -> InfraResult<Self> {
        if lines == 0 || lines > MAX_LINES {
            return Err(InfraError::InvalidRequest {
                domain: "logs",
                message: format!("line count must be 1-{MAX_LINES}"),
            });
        }
        self.lines = lines;
        Ok(self)
    }

    /// Adds a case-sensitive local substring filter.
    pub fn with_grep(mut self, grep: impl Into<String>) -> InfraResult<Self> {
        self.grep = Some(validate_filter("grep", grep.into(), MAX_FILTER_CHARS)?);
        Ok(self)
    }

    /// Sets journal-specific filters.
    pub fn with_journal_filters(mut self, filters: JournalFilters) -> InfraResult<Self> {
        if self.source != LogSource::Journal {
            return Err(InfraError::InvalidRequest {
                domain: "logs",
                message: "journal filters require the journal source".into(),
            });
        }
        self.journal = filters;
        Ok(self)
    }

    /// Returns the source.
    #[must_use]
    pub const fn source(&self) -> LogSource {
        self.source
    }

    /// Returns the line limit.
    #[must_use]
    pub const fn lines(&self) -> u32 {
        self.lines
    }

    /// Returns the optional local substring filter.
    #[must_use]
    pub fn grep(&self) -> Option<&str> {
        self.grep.as_deref()
    }

    /// Returns journal-specific filters.
    #[must_use]
    pub const fn journal(&self) -> &JournalFilters {
        &self.journal
    }

    /// Returns the absolute deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// Structured permission diagnostic for a log source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogPermissionDiagnostic {
    /// Driver-safe failure detail.
    pub message: String,
    /// Operator guidance.
    pub help: String,
}

/// Bounded log read result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRead {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Selected source.
    pub source: LogSource,
    /// Source path when file-backed.
    pub source_path: Option<PathBuf>,
    /// Filtered result lines.
    pub lines: Vec<String>,
    /// Whether the output byte ceiling or line limit omitted data.
    pub truncated: bool,
    /// Permission diagnostic for restricted sources such as dmesg.
    pub permission: Option<LogPermissionDiagnostic>,
}

/// Product-neutral operating-system log reader.
#[async_trait]
pub trait LogReader: Send + Sync {
    /// Reads one bounded log source.
    async fn read_logs(
        &self,
        host: &HostRecord,
        request: &LogReadRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<LogRead>;
}

#[cfg(any(feature = "process-driver", test))]
pub(crate) fn filtered_tail(raw: &str, grep: Option<&str>, limit: u32) -> (Vec<String>, bool) {
    let mut lines = raw
        .lines()
        .filter(|line| grep.is_none_or(|pattern| line.contains(pattern)))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let truncated = lines.len() > limit as usize;
    if truncated {
        lines = lines.split_off(lines.len() - limit as usize);
    }
    (lines, truncated)
}

fn validate_time_filter(value: String) -> InfraResult<String> {
    let option_like = value.starts_with("--")
        || (value.starts_with('-')
            && !value[1..]
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_digit()));
    if value.is_empty()
        || option_like
        || value.chars().count() > MAX_TIME_CHARS
        || value.chars().any(char::is_control)
    {
        Err(InfraError::InvalidRequest {
            domain: "logs",
            message: "journal time filter is invalid or option-like".into(),
        })
    } else {
        Ok(value)
    }
}

fn validate_filter(name: &'static str, value: String, max: usize) -> InfraResult<String> {
    let count = value.chars().count();
    if count == 0 || count > max || value.chars().any(char::is_control) {
        Err(InfraError::InvalidRequest {
            domain: "logs",
            message: format!("{name} must contain 1-{max} printable characters"),
        })
    } else {
        Ok(value)
    }
}

#[cfg(test)]
#[path = "logs_tests.rs"]
mod tests;
