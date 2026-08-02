use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{
    CommandExecutor, CommandOutput, CommandRequest, HostId, HostRecord, TopologyRevision,
};
use soma_ops::Timestamp;
use tokio_util::sync::CancellationToken;

use crate::{InfraError, InfraResult};

const HOST_OUTPUT_LIMIT: usize = 64 * 1024;

/// Deadline-bound request for one host inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostInspectRequest {
    deadline: Timestamp,
}

impl HostInspectRequest {
    /// Creates a host inspection request.
    #[must_use]
    pub const fn new(deadline: Timestamp) -> Self {
        Self { deadline }
    }

    /// Returns the absolute request deadline.
    #[must_use]
    pub const fn deadline(self) -> Timestamp {
        self.deadline
    }
}

/// Stable host identity fields collected from the operating system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostIdentity {
    /// Reported hostname.
    pub hostname: String,
    /// Operating-system family reported by `uname -s`.
    pub operating_system: String,
    /// Kernel release reported by `uname -r`.
    pub kernel_release: String,
    /// Machine architecture reported by `uname -m`.
    pub architecture: String,
}

/// Parsed host memory counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostMemory {
    /// Total physical memory in bytes.
    pub total_bytes: u64,
    /// Currently available memory in bytes.
    pub available_bytes: u64,
    /// Derived used memory in bytes.
    pub used_bytes: u64,
    /// Rounded integer utilization percentage.
    pub usage_percent: u8,
}

/// Parsed Linux load averages.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HostLoadAverage {
    /// One-minute load average.
    pub one: f64,
    /// Five-minute load average.
    pub five: f64,
    /// Fifteen-minute load average.
    pub fifteen: f64,
}

/// Complete read-only host inspection result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostInspection {
    /// Stable fleet host identity.
    pub host: HostId,
    /// Exact topology revision used for collection.
    pub topology_revision: TopologyRevision,
    /// Operating-system identity.
    pub identity: HostIdentity,
    /// Uptime in fractional seconds.
    pub uptime_seconds: f64,
    /// Memory counters.
    pub memory: HostMemory,
    /// Load averages.
    pub load: HostLoadAverage,
}

/// Product-neutral host inspection engine.
#[async_trait]
pub trait HostInspector: Send + Sync {
    /// Collects one typed snapshot from the exact host revision.
    async fn inspect(
        &self,
        host: &HostRecord,
        request: HostInspectRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<HostInspection>;
}

/// Host inspector backed by a `soma-fleet` command executor.
pub struct LinuxCommandHostInspector<E> {
    executor: Arc<E>,
}

impl<E> LinuxCommandHostInspector<E> {
    /// Creates an inspector using the supplied fleet command executor.
    #[must_use]
    pub fn new(executor: Arc<E>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl<E> HostInspector for LinuxCommandHostInspector<E>
where
    E: CommandExecutor,
{
    async fn inspect(
        &self,
        host: &HostRecord,
        request: HostInspectRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<HostInspection> {
        if cancellation.is_cancelled() {
            return Err(soma_fleet::FleetError::Cancelled.into());
        }
        let hostname = self
            .run_text(host, "hostname", &[], request.deadline, cancellation)
            .await?;
        let operating_system = self
            .run_text(host, "uname", &["-s"], request.deadline, cancellation)
            .await?;
        let kernel_release = self
            .run_text(host, "uname", &["-r"], request.deadline, cancellation)
            .await?;
        let architecture = self
            .run_text(host, "uname", &["-m"], request.deadline, cancellation)
            .await?;
        let uptime = self
            .run_text(
                host,
                "cat",
                &["/proc/uptime"],
                request.deadline,
                cancellation,
            )
            .await?;
        let meminfo = self
            .run_text(
                host,
                "cat",
                &["/proc/meminfo"],
                request.deadline,
                cancellation,
            )
            .await?;
        let loadavg = self
            .run_text(
                host,
                "cat",
                &["/proc/loadavg"],
                request.deadline,
                cancellation,
            )
            .await?;

        Ok(HostInspection {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            identity: HostIdentity {
                hostname,
                operating_system,
                kernel_release,
                architecture,
            },
            uptime_seconds: parse_uptime(&uptime)?,
            memory: parse_meminfo(&meminfo)?,
            load: parse_loadavg(&loadavg)?,
        })
    }
}

impl<E> LinuxCommandHostInspector<E>
where
    E: CommandExecutor,
{
    async fn run_text(
        &self,
        host: &HostRecord,
        program: &str,
        args: &[&str],
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<String> {
        let request = CommandRequest::new(program, args.iter().copied(), deadline)
            .map_err(soma_fleet::FleetError::from)?
            .with_output_limits(HOST_OUTPUT_LIMIT, HOST_OUTPUT_LIMIT)
            .map_err(soma_fleet::FleetError::from)?;
        let output = self.executor.execute(host, &request, cancellation).await?;
        checked_text(host, output)
    }
}

fn checked_text(host: &HostRecord, output: CommandOutput) -> InfraResult<String> {
    if output.exit_code() != Some(0) {
        return Err(InfraError::CommandFailed {
            domain: "host",
            host: host.id().clone(),
            exit_code: output.exit_code(),
            stderr: crate::error::public_diagnostic(output.stderr()),
        });
    }
    if output.truncated() {
        return Err(InfraError::Parse {
            domain: "host",
            message: "bounded command output was truncated".into(),
        });
    }
    String::from_utf8(output.stdout().to_vec())
        .map(|value| value.trim().to_owned())
        .map_err(|error| InfraError::Parse {
            domain: "host",
            message: format!("output was not UTF-8: {error}"),
        })
}

fn parse_uptime(raw: &str) -> InfraResult<f64> {
    raw.split_whitespace()
        .next()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or_else(|| InfraError::Parse {
            domain: "host",
            message: "invalid /proc/uptime value".into(),
        })
}

fn parse_meminfo(raw: &str) -> InfraResult<HostMemory> {
    let mut total_kib = None;
    let mut available_kib = None;
    for line in raw.lines() {
        let mut fields = line.split_whitespace();
        let key = fields.next().unwrap_or_default().trim_end_matches(':');
        let value = fields.next().and_then(|value| value.parse::<u64>().ok());
        let unit = fields.next();
        match key {
            "MemTotal" if unit == Some("kB") => total_kib = value,
            "MemAvailable" if unit == Some("kB") => available_kib = value,
            _ => {}
        }
    }
    let total_bytes = total_kib
        .and_then(|value| value.checked_mul(1024))
        .ok_or_else(|| InfraError::Parse {
            domain: "host",
            message: "missing or overflowing MemTotal".into(),
        })?;
    let available_bytes = available_kib
        .and_then(|value| value.checked_mul(1024))
        .ok_or_else(|| InfraError::Parse {
            domain: "host",
            message: "missing or overflowing MemAvailable".into(),
        })?;
    if total_bytes == 0 || available_bytes > total_bytes {
        return Err(InfraError::Parse {
            domain: "host",
            message: "inconsistent MemTotal and MemAvailable".into(),
        });
    }
    let used_bytes = total_bytes - available_bytes;
    let usage_percent = ((used_bytes as f64 / total_bytes as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8;
    Ok(HostMemory {
        total_bytes,
        available_bytes,
        used_bytes,
        usage_percent,
    })
}

fn parse_loadavg(raw: &str) -> InfraResult<HostLoadAverage> {
    let values = raw
        .split_whitespace()
        .take(3)
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| InfraError::Parse {
            domain: "host",
            message: format!("invalid /proc/loadavg value: {error}"),
        })?;
    if values.len() != 3
        || values
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
    {
        return Err(InfraError::Parse {
            domain: "host",
            message: "expected three non-negative load averages".into(),
        });
    }
    Ok(HostLoadAverage {
        one: values[0],
        five: values[1],
        fifteen: values[2],
    })
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
