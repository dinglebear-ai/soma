//! Canonical ingest source-kind contract shared across parsing and persistence.

use serde::{Deserialize, Serialize};

/// Metadata source-kind value for agent-attested Docker identity records.
pub const AGENT_DOCKER_SOURCE_KIND: &str = "agent-docker";

/// Transport or collector that produced an ingest record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    /// UDP syslog listener.
    SyslogUdp,
    /// TCP syslog listener.
    SyslogTcp,
    /// Docker log stream.
    DockerStream,
    /// Docker event stream.
    DockerEvent,
    /// OpenTelemetry ingest.
    Otlp,
    /// AdGuard API collector.
    AdguardApi,
    /// UniFi API collector.
    UnifiApi,
    /// Generic agent ingest.
    Agent,
    /// Local shell-history backfill.
    ShellHistory,
    /// AI agent-launched command spool.
    AgentCommand,
    /// Cortex-managed local file tail.
    FileTail,
}

impl SourceKind {
    /// Every source kind in canonical wire order.
    pub const ALL: [Self; 11] = [
        Self::SyslogUdp,
        Self::SyslogTcp,
        Self::DockerStream,
        Self::DockerEvent,
        Self::Otlp,
        Self::AdguardApi,
        Self::UnifiApi,
        Self::Agent,
        Self::ShellHistory,
        Self::AgentCommand,
        Self::FileTail,
    ];

    /// Canonical kebab-case wire names in stable order.
    pub fn all_wire_names() -> Vec<&'static str> {
        Self::ALL.iter().map(|kind| kind.as_str()).collect()
    }

    /// Stable kebab-case representation stored in metadata and transport contracts.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SyslogUdp => "syslog-udp",
            Self::SyslogTcp => "syslog-tcp",
            Self::DockerStream => "docker-stream",
            Self::DockerEvent => "docker-event",
            Self::Otlp => "otlp",
            Self::AdguardApi => "adguard-api",
            Self::UnifiApi => "unifi-api",
            Self::Agent => "agent",
            Self::ShellHistory => "shell-history",
            Self::AgentCommand => "agent-command",
            Self::FileTail => "file-tail",
        }
    }

    /// Parse a canonical kebab-case wire value.
    pub fn from_wire(value: &str) -> Option<Self> {
        let trimmed = value.trim();
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == trimmed)
    }

    /// Whether this source is one of the two syslog listener transports.
    pub const fn is_syslog(self) -> bool {
        matches!(self, Self::SyslogUdp | Self::SyslogTcp)
    }
}

#[cfg(test)]
#[path = "source_kind_tests.rs"]
mod tests;
