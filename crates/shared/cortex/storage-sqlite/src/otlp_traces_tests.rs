use super::*;

#[test]
fn span_row_json_round_trip_preserves_trace_identity() {
    let row = OtelSpanRow {
        id: 1,
        trace_id: "trace".into(),
        span_id: "span".into(),
        parent_span_id: Some("parent".into()),
        trace_state: None,
        flags: 1,
        span_name: "request".into(),
        span_kind: 2,
        start_time_unix_nano: 10,
        end_time_unix_nano: 20,
        duration_nano: 10,
        status_code: 1,
        status_message: None,
        hostname: "dookie".into(),
        service_name: Some("soma".into()),
        service_version: None,
        scope_name: None,
        scope_version: None,
        ai_tool: None,
        ai_project: None,
        ai_session_id: None,
        run_id: None,
        resource_json: "{}".into(),
        attributes_json: "{}".into(),
        events_json: "[]".into(),
        links_json: "[]".into(),
        received_at: "2026-08-18T00:00:00Z".into(),
        content_scrubbed: true,
    };
    let decoded: OtelSpanRow = serde_json::from_str(&serde_json::to_string(&row).unwrap()).unwrap();
    assert_eq!(decoded, row);
}
