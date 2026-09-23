use super::*;

#[test]
fn entity_summary_preserves_domain_identity() {
    let entity = GraphEntity {
        id: 7,
        entity_type: "host".into(),
        canonical_key: "dookie".into(),
        display_label: "DOOKIE".into(),
        source_kind: "inventory".into(),
        source_id: "host:dookie".into(),
        trust_level: "observed".into(),
        first_seen_at: None,
        last_seen_at: None,
    };
    let summary = GraphEntitySummary::from(&entity);
    assert_eq!(summary.id, 7);
    assert_eq!(summary.canonical_key, "dookie");
    assert_eq!(summary.trust_level, "observed");
}
