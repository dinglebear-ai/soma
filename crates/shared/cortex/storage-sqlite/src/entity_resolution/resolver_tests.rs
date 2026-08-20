use super::*;

fn observation(kind: ObservationKind) -> ResolverObservation {
    ResolverObservation {
        kind,
        observed_key: "plex".into(),
        display_label: "Plex".into(),
        host_key: Some("nas".into()),
        logical_service_key: Some("plex".into()),
        service_instance_key: Some("nas/plex".into()),
        source_kind: "app_inventory".into(),
        source_id: "service:nas:plex".into(),
        evidence_path: "inventory.services".into(),
        observed_at: "2026-08-18T00:00:00Z".into(),
        trust: ResolverTrust::Verified,
        structured: true,
    }
}

#[test]
fn raw_labels_never_self_upgrade_but_structured_instances_do() {
    assert!(resolve_observations(&[observation(ObservationKind::RawAppLabel)]).is_empty());
    let decisions = resolve_observations(&[observation(ObservationKind::ServiceInstance)]);
    assert_eq!(decisions.len(), 2);
    assert!(
        decisions
            .iter()
            .any(|d| d.entity_type == ENTITY_TYPE_LOGICAL_SERVICE && d.canonical_key == "plex")
    );
    assert!(decisions.iter().any(|d| d.entity_type == ENTITY_TYPE_SERVICE_INSTANCE && d.canonical_key == "nas/plex"));
}

#[test]
fn lookup_diagnostics_reject_legacy_shapes() {
    assert_eq!(
        diagnose_lookup_input("nas:plex").status,
        ResolverStatus::RejectedLegacyShape
    );
    assert_eq!(
        diagnose_lookup_input("plex").status,
        ResolverStatus::Degraded
    );
}
