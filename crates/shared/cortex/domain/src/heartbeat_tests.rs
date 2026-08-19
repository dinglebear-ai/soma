use super::*;

#[test]
fn heartbeat_flags_default_to_no_pressure() {
    let flags = HeartbeatStateFlags::default();
    let value = serde_json::to_value(flags).unwrap();
    assert!(
        value
            .as_object()
            .unwrap()
            .values()
            .all(|value| value == false)
    );
}

#[test]
fn heartbeat_window_summary_round_trips_without_storage_types() {
    let json = serde_json::json!({
        "host_id":"host-1", "hostname":"dookie", "samples":4, "partial_samples":1,
        "max_cpu_usage_percent":72.5, "min_mem_available_bytes":1048576,
        "pressure_flags":["cpu_pressure"]
    });
    let summary: HeartbeatWindowSummary = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(summary.samples, 4);
    assert_eq!(serde_json::to_value(summary).unwrap(), json);
}

#[test]
fn heartbeat_policy_matches_locked_pressure_semantics() {
    let sample = HeartbeatSampleState {
        heartbeat_id: 1,
        host_id: "host-1".into(),
        hostname: "dookie".into(),
        sampled_at: "1970-01-01T00:00:00Z".into(),
        received_at: "1970-01-01T00:00:00Z".into(),
        source_ip: "127.0.0.1".into(),
        boot_id: "boot-1".into(),
        sequence: 1,
        uptime_secs: 1,
        collection_ms: 1,
        partial: false,
        agent_version: "test".into(),
        os: "linux".into(),
        kernel: None,
        architecture: "x86_64".into(),
        metadata: Some(serde_json::json!({"agent":{"interval_secs":30}})),
        cpu: Some(serde_json::json!({"usage_percent":91.0})),
        memory: Some(serde_json::json!({
            "used_percent": 89.0, "swap_total_bytes": 100, "swap_used_bytes": 91
        })),
        disks: vec![
            serde_json::json!({"filesystem":"iso9660","mountpoint":"/snap/x","used_percent":100.0}),
            serde_json::json!({"filesystem":"ext4","mountpoint":"/","used_percent":95.0}),
        ],
        network: vec![serde_json::json!({"rx_errors":1,"tx_errors":0})],
        processes: None,
        containers: vec![serde_json::json!({"unhealthy":1})],
    };

    let flags = heartbeat_flags_from_sample(&sample);
    assert!(flags.heartbeat_late);
    assert!(flags.cpu_pressure);
    assert!(!flags.memory_pressure);
    assert!(flags.swap_pressure);
    assert!(flags.disk_capacity_pressure);
    assert!(flags.network_error_pressure);
    assert!(flags.container_unhealthy);
    assert_eq!(heartbeat_host_status_label(&flags), "late");
    assert_eq!(
        heartbeat_pressure_names(&flags),
        vec![
            "cpu_pressure",
            "swap_pressure",
            "disk_capacity_pressure",
            "network_error_pressure",
            "container_unhealthy",
        ]
    );
}
