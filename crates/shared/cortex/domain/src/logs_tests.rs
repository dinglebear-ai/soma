use super::*;

fn sample() -> LogEntry {
    LogEntry {
        id: 42,
        timestamp: "2026-01-01T00:00:00Z".into(),
        hostname: "claimed-host".into(),
        facility: Some("local0".into()),
        severity: "warning".into(),
        app_name: Some("rsyslogd".into()),
        process_id: Some("123".into()),
        message: "message".into(),
        received_at: "2026-01-01T00:00:01Z".into(),
        source_ip: "192.0.2.10:514".into(),
        ai_tool: None,
        ai_project: None,
        ai_session_id: None,
        ai_transcript_path: None,
        metadata_json: None,
    }
}

#[test]
fn log_entry_wire_shape_preserves_network_sender_identity() {
    let value = serde_json::to_value(sample()).unwrap();
    assert_eq!(value["hostname"], "claimed-host");
    assert_eq!(value["source_ip"], "192.0.2.10:514");
    assert_eq!(value["app_name"], "rsyslogd");
}
