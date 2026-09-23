use super::*;
use cortex_inventory::{MountRef, Provenance};

fn service(host: Option<&str>) -> InventoryService {
    InventoryService {
        id: "service:nas:plex".into(),
        name: "Plex".into(),
        kind: "container".into(),
        trust_level: TrustLevel::Observed,
        provenance: Provenance::new("docker:nas", "app_inventory", "2026-08-18T00:00:00Z".into()),
        host: host.map(str::to_owned),
        image: None,
        status: Some("running".into()),
        domains: vec!["plex.example.test".into()],
        ports: Vec::new(),
        mounts: vec![MountRef {
            source: None,
            target: "/media".into(),
            read_only: true,
        }],
        env_keys: Vec::new(),
        labels: Default::default(),
        details: Default::default(),
    }
}

#[test]
fn hostless_inventory_service_asserts_only_logical_identity() {
    let observations = observations_from_inventory_service(&service(None));
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].kind, ObservationKind::LogicalService);
}

#[test]
fn hosted_inventory_service_emits_topology_context() {
    let observations = observations_from_inventory_service(&service(Some("NAS")));
    assert!(
        observations
            .iter()
            .any(|o| o.kind == ObservationKind::ServiceInstance && o.observed_key == "nas/plex")
    );
    assert!(
        observations
            .iter()
            .any(|o| o.kind == ObservationKind::Domain)
    );
    assert!(
        observations
            .iter()
            .any(|o| o.kind == ObservationKind::Storage)
    );
    assert!(
        observations
            .iter()
            .all(|o| o.trust == ResolverTrust::Verified)
    );
}
