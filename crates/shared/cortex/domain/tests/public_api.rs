use cortex_domain::{
    DomainError, GraphEntity, GraphEntitySummary, HeartbeatStateFlags, RequestActor,
    graph_confidence, hook_signal_detectors, mcp_signal_detectors, observatory_identity,
    skill_signal_detectors, topology_findings,
};

#[test]
fn independent_consumer_can_use_domain_without_cortex_runtime() {
    let actor = RequestActor::mcp_identity(Some("sub".into()), Some("user@example.com".into()));
    assert_eq!(actor.display, "user@example.com");

    let entity = GraphEntity {
        id: 1,
        entity_type: "host".into(),
        canonical_key: "dookie".into(),
        display_label: "DOOKIE".into(),
        source_kind: "inventory".into(),
        source_id: "host:dookie".into(),
        trust_level: "observed".into(),
        first_seen_at: None,
        last_seen_at: None,
    };
    assert_eq!(GraphEntitySummary::from(&entity).canonical_key, "dookie");
    assert!(!HeartbeatStateFlags::default().cpu_pressure);
    assert!(topology_findings::TYPES.contains(&topology_findings::TYPE_RISKY_MOUNTS));
    assert!(hook_signal_detectors::is_hook_failure_status("failed"));
    assert!((graph_confidence::noisy_or_combine(&[0.5, 0.5]) - 0.75).abs() < 1e-9);
    assert!(mcp_signal_detectors::detect_timeout_or_rate_limit(
        "tool timed out"
    ));
    assert!(skill_signal_detectors::detect_tool_failure(
        "command failed to start"
    ));
    assert_eq!(
        observatory_identity::run_key("dookie", "Codex", "session-1").unwrap(),
        "v1|6:dookie|5:codex|9:session-1"
    );
    assert_eq!(
        DomainError::NotFound("missing".into()).to_string(),
        "missing"
    );
}

#[test]
fn public_api_manifest_has_no_product_specific_dependencies() {
    let manifest =
        std::fs::read_to_string(format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR"))).unwrap();
    for forbidden in ["rusqlite", "r2d2", "axum", "rmcp", "lab-auth", "soma-auth"] {
        assert!(
            !manifest.contains(forbidden),
            "unexpected dependency: {forbidden}"
        );
    }
}
