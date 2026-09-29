use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod topology_findings {
    pub const TYPE_POTENTIAL_PUBLIC_ROUTE: &str = "potential_public_route";
    pub const TYPE_RISKY_MOUNTS: &str = "risky_mounts";
    pub const TYPE_COLLECTOR_HEALTH: &str = "collector_health";
    pub const TYPES: [&str; 3] = [
        TYPE_POTENTIAL_PUBLIC_ROUTE,
        TYPE_RISKY_MOUNTS,
        TYPE_COLLECTOR_HEALTH,
    ];

    pub const SEVERITY_CRITICAL: &str = "critical";
    pub const SEVERITY_HIGH: &str = "high";
    pub const SEVERITY_MEDIUM: &str = "medium";
    pub const SEVERITY_LOW: &str = "low";
    pub const SEVERITY_INFO: &str = "info";

    pub mod reason {
        pub const REVERSE_PROXY_ROUTE_CONFIGURED: &str = "reverse_proxy_route_configured";
        pub const REVERSE_PROXY_DOMAIN_WITHOUT_TARGET_PROOF: &str =
            "reverse_proxy_domain_without_target_proof";
        pub const DOCKER_SOCKET_MOUNT: &str = "docker_socket_mount";
        pub const HOST_ROOT_MOUNT: &str = "host_root_mount";
        pub const APPDATA_ROOT_MOUNT: &str = "appdata_root_mount";
        pub const MOUNT_MISSING_SOURCE_DETAIL: &str = "mount_missing_source_detail";
        pub const GRAPH_PROJECTION_NOT_READY: &str = "graph_projection_not_ready";
        pub const INVENTORY_CACHE_MISSING: &str = "inventory_cache_missing";
        pub const INVENTORY_CACHE_STALE: &str = "inventory_cache_stale";
        pub const INVENTORY_CACHE_UNREADABLE: &str = "inventory_cache_unreadable";
        pub const COLLECTION_STATE_UNAVAILABLE: &str = "collection_state_unavailable";
        pub const COLLECTOR_DEGRADED: &str = "collector_degraded";
        pub const COLLECTOR_PARTIAL: &str = "collector_partial";
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyFinding {
    pub finding_type: String,
    pub severity: String,
    pub confidence: f64,
    pub reason_code: String,
    pub affected_entities: Vec<TopologyFindingEntity>,
    pub evidence: Vec<TopologyFindingEvidence>,
    /// Total safe evidence items available before per-finding and payload
    /// budget limits were applied.
    pub evidence_total: usize,
    /// True when safe evidence was omitted from this finding.
    pub evidence_truncated: bool,
    /// Number of safe evidence items omitted from this finding.
    pub evidence_omitted: usize,
    pub remediation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degraded_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyFindingEntity {
    pub entity_type: String,
    pub key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyFindingEvidence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_id: Option<i64>,
    pub source_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safe_excerpt: Option<String>,
}

#[cfg(test)]
#[path = "topology_tests.rs"]
mod tests;
