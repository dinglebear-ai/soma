use super::*;

#[test]
fn actor_tie_break_prefers_newer_activity_then_stable_payload_order() {
    let row = AgentActorRow {
        id: 1,
        actor_key: "actor".into(),
        run_id: 1,
        native_actor_id: "main".into(),
        actor_type: Some("primary".into()),
        display_name: Some("Agent".into()),
        started_at: Some("2026-08-18T00:00:00Z".into()),
        last_activity_at: Some("2026-08-18T00:01:00Z".into()),
        ended_at: None,
        metadata_json: "{}".into(),
    };
    let newer = AgentActorUpsert {
        native_actor_id: "main".into(),
        actor_type: Some("primary".into()),
        display_name: Some("Agent".into()),
        started_at: row.started_at.clone(),
        last_activity_at: Some("2026-08-18T00:02:00Z".into()),
        ended_at: None,
        metadata_json: "{}".into(),
    };
    assert!(actor_wins(&newer, &row));

    let older = AgentActorUpsert {
        last_activity_at: Some("2026-08-18T00:00:30Z".into()),
        ..newer
    };
    assert!(!actor_wins(&older, &row));
}
