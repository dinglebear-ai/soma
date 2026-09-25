use serde_json::json;
use soma_fleet::{HostEndpoint, HostId};

use super::*;

fn host() -> HostRecord {
    HostRecord::new(HostId::new("devhost").unwrap(), HostEndpoint::Local)
}

#[test]
fn disk_usage_mapping_aggregates_stable_categories() {
    let value = json!({
        "LayersSize": 10,
        "Images": [{"Size": 100}, {"Size": 200}],
        "Containers": [{"SizeRw": 30}],
        "Volumes": [{"UsageData": {"Size": 40}}],
        "BuildCache": [{"Size": 50}, {"Size": 60}]
    });
    let usage = map_disk_usage(&host(), &value).unwrap();
    assert_eq!(usage.layers_size_bytes, 10);
    assert_eq!(usage.images.count, 2);
    assert_eq!(usage.images.size_bytes, 300);
    assert_eq!(usage.containers.size_bytes, 30);
    assert_eq!(usage.volumes.size_bytes, 40);
    assert_eq!(usage.build_cache.size_bytes, 110);
}

#[test]
fn stats_mapping_aggregates_network_and_block_io() {
    let value = json!({
        "read": "2026-08-01T00:00:00Z",
        "pids_stats": {"current": 7},
        "memory_stats": {"usage": 100, "limit": 1000},
        "cpu_stats": {
            "cpu_usage": {"total_usage": 123},
            "system_cpu_usage": 456,
            "online_cpus": 8
        },
        "networks": {
            "eth0": {"rx_bytes": 10, "tx_bytes": 20},
            "eth1": {"rx_bytes": 30, "tx_bytes": 40}
        },
        "blkio_stats": {
            "io_service_bytes_recursive": [
                {"op": "Read", "value": 50},
                {"op": "Write", "value": 60}
            ]
        }
    });
    let stats = map_container_stats(&host(), "api", &value).unwrap();
    assert_eq!(stats.pids_current, 7);
    assert_eq!(stats.network_rx_bytes, 40);
    assert_eq!(stats.network_tx_bytes, 60);
    assert_eq!(stats.block_read_bytes, 50);
    assert_eq!(stats.block_write_bytes, 60);
}
