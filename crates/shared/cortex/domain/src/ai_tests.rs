use super::*;

#[test]
fn abuse_incident_wire_shape_matches_donor_fields() {
    let incident = AbuseIncident {
        incident_id: "inc-1".into(),
        project: "/tmp/project".into(),
        tool: "claude".into(),
        session_id: "sess-1".into(),
        hostname: "dookie".into(),
        first_seen: "2026-01-01T00:00:00Z".into(),
        last_seen: "2026-01-01T00:05:00Z".into(),
        duration_secs: 300,
        abuse_count: 2,
        terms: vec!["term".into()],
        anchor_ids: vec![1, 2],
        priority_score: 0.8,
        priority_label: "high".into(),
        window_minutes: 10,
    };
    let value = serde_json::to_value(incident).unwrap();
    assert_eq!(value["incident_id"], "inc-1");
    assert_eq!(value["anchor_ids"], serde_json::json!([1, 2]));
}
