use super::*;

#[test]
fn actor_upsert_serializes_stable_public_fields() {
    let actor = AgentActorUpsert {
        native_actor_id: "main".into(),
        actor_type: Some("primary".into()),
        display_name: Some("Main agent".into()),
        started_at: Some("2026-08-18T00:00:00Z".into()),
        last_activity_at: Some("2026-08-18T00:01:00Z".into()),
        ended_at: None,
        metadata_json: "{}".into(),
    };
    let value = serde_json::to_value(actor).unwrap();
    assert_eq!(value["native_actor_id"], "main");
    assert_eq!(value["actor_type"], "primary");
    assert_eq!(value["metadata_json"], "{}");
}
