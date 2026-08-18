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
