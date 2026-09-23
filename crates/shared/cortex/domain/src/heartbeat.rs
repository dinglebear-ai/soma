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
/// Flag computation is owned by [`heartbeat_flags_from_sample`] so every
/// transport and storage adapter shares identical thresholds and logic.
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

const CPU_PRESSURE_THRESHOLD: f64 = 90.0;
const MEM_PRESSURE_THRESHOLD: f64 = 90.0;
const SWAP_PRESSURE_RATIO: f64 = 0.9;
const DISK_CAPACITY_THRESHOLD: f64 = 90.0;
const LATE_MULTIPLIER_MS: i64 = 2500;
const CLOCK_SKEW_THRESHOLD_SECS: i64 = 30;

/// Derive canonical heartbeat state flags from a fully-loaded sample.
pub fn heartbeat_flags_from_sample(sample: &HeartbeatSampleState) -> HeartbeatStateFlags {
    let interval_secs = sample
        .metadata
        .as_ref()
        .and_then(|m| m.pointer("/agent/interval_secs"))
        .and_then(Value::as_i64)
        .unwrap_or(30)
        .max(1);

    let max_disk = sample
        .disks
        .iter()
        .filter_map(disk_pressure_used_percent)
        .fold(None::<f64>, |acc, value| {
            Some(acc.map_or(value, |current| current.max(value)))
        });
    let network_errors: i64 = sample
        .network
        .iter()
        .map(|network| {
            network["rx_errors"].as_i64().unwrap_or(0) + network["tx_errors"].as_i64().unwrap_or(0)
        })
        .sum();

    HeartbeatStateFlags {
        collector_partial: sample.partial,
        heartbeat_late: compute_late(&sample.received_at, interval_secs),
        clock_skew: compute_clock_skew(&sample.sampled_at, &sample.received_at),
        cpu_pressure: sample
            .cpu
            .as_ref()
            .and_then(|cpu| cpu["usage_percent"].as_f64())
            .is_some_and(|percent| percent > CPU_PRESSURE_THRESHOLD),
        memory_pressure: sample
            .memory
            .as_ref()
            .and_then(|memory| memory["used_percent"].as_f64())
            .is_some_and(|percent| percent > MEM_PRESSURE_THRESHOLD),
        swap_pressure: swap_ratio(
            sample
                .memory
                .as_ref()
                .and_then(|memory| memory["swap_total_bytes"].as_i64()),
            sample
                .memory
                .as_ref()
                .and_then(|memory| memory["swap_used_bytes"].as_i64()),
        ),
        disk_capacity_pressure: max_disk.is_some_and(|percent| percent > DISK_CAPACITY_THRESHOLD),
        network_error_pressure: network_errors > 0,
        container_unhealthy: sample
            .containers
            .iter()
            .any(|container| container["unhealthy"].as_i64().unwrap_or(0) > 0),
    }
}

/// Return active resource-pressure signal names in canonical order.
pub fn heartbeat_pressure_names(flags: &HeartbeatStateFlags) -> Vec<String> {
    let mut names = Vec::new();
    if flags.cpu_pressure {
        names.push("cpu_pressure".to_owned());
    }
    if flags.memory_pressure {
        names.push("memory_pressure".to_owned());
    }
    if flags.swap_pressure {
        names.push("swap_pressure".to_owned());
    }
    if flags.disk_capacity_pressure {
        names.push("disk_capacity_pressure".to_owned());
    }
    if flags.network_error_pressure {
        names.push("network_error_pressure".to_owned());
    }
    if flags.container_unhealthy {
        names.push("container_unhealthy".to_owned());
    }
    names
}

/// Canonical fleet status label. Priority is late, partial, pressure, then ok.
pub fn heartbeat_host_status_label(flags: &HeartbeatStateFlags) -> &'static str {
    let has_pressure = flags.cpu_pressure
        || flags.memory_pressure
        || flags.swap_pressure
        || flags.disk_capacity_pressure
        || flags.network_error_pressure
        || flags.container_unhealthy;
    if flags.heartbeat_late {
        "late"
    } else if flags.collector_partial {
        "partial"
    } else if has_pressure {
        "pressure"
    } else {
        "ok"
    }
}

fn compute_late(received_at: &str, interval_secs: i64) -> bool {
    chrono::DateTime::parse_from_rfc3339(received_at).is_ok_and(|dt| {
        let elapsed = chrono::Utc::now().signed_duration_since(dt.with_timezone(&chrono::Utc));
        elapsed.num_milliseconds() > interval_secs.max(1) * LATE_MULTIPLIER_MS
    })
}

fn compute_clock_skew(sampled_at: &str, received_at: &str) -> bool {
    let sampled = chrono::DateTime::parse_from_rfc3339(sampled_at).ok();
    let received = chrono::DateTime::parse_from_rfc3339(received_at).ok();
    match (sampled, received) {
        (Some(sampled), Some(received)) => {
            let skew = sampled.with_timezone(&chrono::Utc) - received.with_timezone(&chrono::Utc);
            skew.num_seconds().abs() > CLOCK_SKEW_THRESHOLD_SECS
        }
        _ => false,
    }
}

fn swap_ratio(swap_total: Option<i64>, swap_used: Option<i64>) -> bool {
    match (swap_total, swap_used) {
        (Some(total), Some(used)) if total > 0 => {
            (used as f64 / total as f64) > SWAP_PRESSURE_RATIO
        }
        _ => false,
    }
}

fn disk_pressure_used_percent(disk: &Value) -> Option<f64> {
    is_pressure_relevant_disk(disk)
        .then(|| disk["used_percent"].as_f64())
        .flatten()
}

fn is_pressure_relevant_disk(disk: &Value) -> bool {
    let filesystem = disk["filesystem"]
        .as_str()
        .or_else(|| disk["fs_type"].as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mount = disk["mountpoint"]
        .as_str()
        .or_else(|| disk["name"].as_str())
        .unwrap_or("");

    if matches!(
        filesystem.as_str(),
        "autofs"
            | "binfmt_misc"
            | "bpf"
            | "cgroup"
            | "cgroup2"
            | "configfs"
            | "debugfs"
            | "devpts"
            | "devtmpfs"
            | "efivarfs"
            | "fuse.snapfuse"
            | "fusectl"
            | "hugetlbfs"
            | "iso9660"
            | "mqueue"
            | "nsfs"
            | "overlay"
            | "proc"
            | "pstore"
            | "ramfs"
            | "rootfs"
            | "securityfs"
            | "squashfs"
            | "sysfs"
            | "tmpfs"
            | "tracefs"
    ) {
        return false;
    }

    !matches!(mount, "" | "/init")
        && !mount.starts_with("/snap/")
        && !mount.starts_with("/mnt/wsl/docker-desktop/")
        && !mount.starts_with("/mnt/wslg/")
        && !mount.starts_with("/usr/lib/modules/")
        && !mount.starts_with("/usr/lib/wsl/")
        && !mount.starts_with("/run/")
        && !mount.starts_with("/var/run/")
}

#[cfg(test)]
#[path = "heartbeat_tests.rs"]
mod tests;
