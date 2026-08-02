use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use soma_fleet::{HostId, HostRecord, TopologyRevision};
use soma_ops::Timestamp;
use tokio_util::sync::CancellationToken;

use crate::{InfraError, InfraResult};

const MAX_FILTER_CHARS: usize = 256;
const MAX_PORT_ROWS: u32 = 5000;

/// Request for a bounded service listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceListRequest {
    service: Option<String>,
    state: Option<String>,
    deadline: Timestamp,
}

impl ServiceListRequest {
    /// Creates an unfiltered service request.
    #[must_use]
    pub const fn new(deadline: Timestamp) -> Self {
        Self {
            service: None,
            state: None,
            deadline,
        }
    }
    /// Filters by service-name substring.
    pub fn with_service(mut self, value: impl Into<String>) -> InfraResult<Self> {
        self.service = Some(validate_filter("service", value.into())?);
        Ok(self)
    }
    /// Filters by active-state equality.
    pub fn with_state(mut self, value: impl Into<String>) -> InfraResult<Self> {
        self.state = Some(validate_filter("state", value.into())?);
        Ok(self)
    }
    /// Returns the service filter.
    #[must_use]
    pub fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }
    /// Returns the state filter.
    #[must_use]
    pub fn state(&self) -> Option<&str> {
        self.state.as_deref()
    }
    /// Returns the absolute deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// One system service row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceStatus {
    /// Unit name.
    pub unit: String,
    /// Load state.
    pub load: String,
    /// Active state.
    pub active: String,
    /// Sub-state.
    pub sub: String,
    /// Description.
    pub description: String,
}

/// One interface address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkAddress {
    /// Address family.
    pub family: String,
    /// Address text.
    pub address: String,
    /// Prefix length.
    pub prefix_len: u8,
}

/// One network interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkInterface {
    /// Interface index.
    pub index: u64,
    /// Interface name.
    pub name: String,
    /// Operational state.
    pub state: Option<String>,
    /// MTU.
    pub mtu: Option<u64>,
    /// Interface addresses.
    pub addresses: Vec<NetworkAddress>,
}

/// One mounted filesystem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountInfo {
    /// Mount target.
    pub target: String,
    /// Source device or dataset.
    pub source: Option<String>,
    /// Filesystem type.
    pub filesystem: Option<String>,
    /// Mount options.
    pub options: Option<String>,
    /// Total bytes.
    pub size_bytes: Option<u64>,
    /// Used bytes.
    pub used_bytes: Option<u64>,
    /// Available bytes.
    pub available_bytes: Option<u64>,
}

/// Supported listening-port protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortProtocol {
    /// TCP sockets.
    Tcp,
    /// UDP sockets.
    Udp,
}

impl PortProtocol {
    #[cfg(feature = "process-driver")]
    pub(crate) const fn as_ss_filter(self) -> &'static str {
        match self {
            Self::Tcp => "-t",
            Self::Udp => "-u",
        }
    }
}

/// Request for bounded listening-port inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortListRequest {
    protocol: Option<PortProtocol>,
    offset: u32,
    limit: u32,
    deadline: Timestamp,
}

impl PortListRequest {
    /// Creates a request returning up to 500 rows.
    #[must_use]
    pub const fn new(deadline: Timestamp) -> Self {
        Self {
            protocol: None,
            offset: 0,
            limit: 500,
            deadline,
        }
    }
    /// Restricts the protocol.
    #[must_use]
    pub const fn with_protocol(mut self, protocol: PortProtocol) -> Self {
        self.protocol = Some(protocol);
        self
    }
    /// Sets pagination bounds.
    pub fn with_page(mut self, offset: u32, limit: u32) -> InfraResult<Self> {
        if limit == 0 || limit > MAX_PORT_ROWS {
            return Err(InfraError::InvalidRequest {
                domain: "host",
                message: format!("port limit must be 1-{MAX_PORT_ROWS}"),
            });
        }
        self.offset = offset;
        self.limit = limit;
        Ok(self)
    }
    /// Returns the protocol filter.
    #[must_use]
    pub const fn protocol(&self) -> Option<PortProtocol> {
        self.protocol
    }
    /// Returns the row offset.
    #[must_use]
    pub const fn offset(&self) -> u32 {
        self.offset
    }
    /// Returns the row limit.
    #[must_use]
    pub const fn limit(&self) -> u32 {
        self.limit
    }
    /// Returns the deadline.
    #[must_use]
    pub const fn deadline(&self) -> Timestamp {
        self.deadline
    }
}

/// One listening socket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortInfo {
    /// Protocol name.
    pub protocol: String,
    /// Socket state.
    pub state: String,
    /// Local address.
    pub local_address: String,
    /// Peer address.
    pub peer_address: String,
    /// Process annotation.
    pub process: Option<String>,
}

/// Byte-precise filesystem usage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesystemUsage {
    /// Source device or dataset.
    pub source: String,
    /// Filesystem type.
    pub filesystem: String,
    /// Total bytes.
    pub size_bytes: u64,
    /// Used bytes.
    pub used_bytes: u64,
    /// Available bytes.
    pub available_bytes: u64,
    /// Integer utilization percentage.
    pub usage_percent: u8,
    /// Mount target.
    pub target: String,
}

/// One doctor check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorCheck {
    /// Stable check name.
    pub name: String,
    /// Whether the check passed.
    pub ok: bool,
    /// Human-readable summary.
    pub summary: String,
}

/// Typed doctor report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorReport {
    /// Target host.
    pub host: HostId,
    /// Exact topology revision.
    pub topology_revision: TopologyRevision,
    /// Overall status.
    pub overall: String,
    /// Individual checks.
    pub checks: Vec<DoctorCheck>,
}

/// Remaining product-neutral host-system reads.
#[async_trait]
pub trait HostSystemInspector: Send + Sync {
    /// Lists services.
    async fn services(
        &self,
        host: &HostRecord,
        request: &ServiceListRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ServiceStatus>>;
    /// Reads network interfaces.
    async fn network(
        &self,
        host: &HostRecord,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<NetworkInterface>>;
    /// Lists mounted filesystems.
    async fn mounts(
        &self,
        host: &HostRecord,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<MountInfo>>;
    /// Lists listening ports.
    async fn ports(
        &self,
        host: &HostRecord,
        request: &PortListRequest,
        cancellation: &CancellationToken,
    ) -> InfraResult<Vec<PortInfo>>;
    /// Reads byte-precise filesystem usage.
    async fn filesystem_usage(
        &self,
        host: &HostRecord,
        path: Option<&str>,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<FilesystemUsage>;
    /// Runs deterministic read-only health checks.
    async fn doctor(
        &self,
        host: &HostRecord,
        deadline: Timestamp,
        cancellation: &CancellationToken,
    ) -> InfraResult<DoctorReport>;
}

fn validate_filter(field: &'static str, value: String) -> InfraResult<String> {
    if value.is_empty()
        || value.chars().count() > MAX_FILTER_CHARS
        || value.chars().any(char::is_control)
    {
        Err(InfraError::InvalidRequest {
            domain: "host",
            message: format!("invalid {field} filter"),
        })
    } else {
        Ok(value)
    }
}

#[cfg(test)]
#[path = "host_system_tests.rs"]
mod tests;
