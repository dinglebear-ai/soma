use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatHostState {
    pub host_id: String,
    pub hostname: String,
    pub total_samples: usize,
    pub truncated: bool,
    pub flags: HeartbeatStateFlags,
    pub latest: Option<HeartbeatSampleState>,
    pub samples: Vec<HeartbeatSampleState>,
}

/// Server-computed derived signals for a heartbeat sample.
/// These are the canonical source of truth for fleet views and correlation;
/// agent-supplied local flags are informational only.
///
/// All flag computation is centralised in `app::heartbeat_flags::derive_flags`
/// so that MCP, REST, and CLI adapters share identical thresholds and logic.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HeartbeatStateFlags {
    // -- Availability ---------------------------------------------------------
    pub collector_partial: bool,
    pub heartbeat_late: bool,
    pub clock_skew: bool,
    // -- Resource pressure ----------------------------------------------------
    pub cpu_pressure: bool,
    pub memory_pressure: bool,
    pub swap_pressure: bool,
    pub disk_capacity_pressure: bool,
    pub network_error_pressure: bool,
    pub container_unhealthy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatSampleState {
    pub heartbeat_id: i64,
    pub host_id: String,
    pub hostname: String,
    pub sampled_at: String,
    pub received_at: String,
    pub source_ip: String,
    pub boot_id: String,
    pub sequence: i64,
    pub uptime_secs: i64,
    pub collection_ms: i64,
    pub partial: bool,
    pub agent_version: String,
    pub os: String,
    pub kernel: Option<String>,
    pub architecture: String,
    pub metadata: Option<Value>,
    pub cpu: Option<Value>,
    pub memory: Option<Value>,
    pub disks: Vec<Value>,
    pub network: Vec<Value>,
    pub processes: Option<Value>,
    pub containers: Vec<Value>,
}

/// Return all heartbeat rows for `host_id` within `[from, to]` (inclusive),
/// with lightweight summaries for `correlate_state`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatWindowSummary {
    pub host_id: String,
    pub hostname: String,
    pub samples: usize,
    pub partial_samples: usize,
    pub max_cpu_usage_percent: Option<f64>,
    pub min_mem_available_bytes: Option<i64>,
    pub pressure_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetStateHostRow {
    pub host_id: String,
    pub hostname: String,
    pub last_heartbeat_at: String,
    pub status: String,
    pub pressure: Vec<String>,
    pub partial: bool,
    pub clock_skew: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FleetStateSummary {
    pub total: usize,
    pub ok: usize,
    pub late: usize,
    pub partial: usize,
    pub pressure: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelateStateWindow {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelateStateHostEntry {
    pub host_id: String,
    pub hostname: String,
    pub heartbeat_summary: HeartbeatWindowSummary,
    pub logs: Vec<crate::LogEntry>,
}

#[cfg(test)]
#[path = "heartbeat_tests.rs"]
mod tests;
