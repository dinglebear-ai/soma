use super::*;

#[test]
fn identities_validate_and_round_trip() {
    let host = HostId::new("dookie").unwrap();
    let capability = CapabilityName::new("transport.ssh").unwrap();
    let revision = TopologyRevision::from_material(b"dookie:ssh:22");
    let key = PoolKey::new(host.clone(), revision.clone());

    assert_eq!(host.as_str(), "dookie");
    assert_eq!(capability.as_str(), "transport.ssh");
    assert_eq!(revision.as_str().len(), 64);
    assert_eq!(key.host(), &host);
    assert_eq!(key.revision(), &revision);
    assert!(key.to_string().starts_with("dookie@"));

    let encoded = serde_json::to_string(&key).unwrap();
    assert_eq!(serde_json::from_str::<PoolKey>(&encoded).unwrap(), key);
}

#[test]
fn invalid_identifiers_fail_closed() {
    for invalid in ["", "Dookie", "host.name", "bad/host", "host-"] {
        assert!(HostId::new(invalid).is_err(), "accepted {invalid}");
    }
    for invalid in ["ssh", "Transport.ssh", "transport..ssh", "transport.ssh-"] {
        assert!(CapabilityName::new(invalid).is_err(), "accepted {invalid}");
    }
    assert!(TopologyRevision::new("a".repeat(63)).is_err());
    assert!(TopologyRevision::new("G".repeat(64)).is_err());
    assert!(serde_json::from_str::<HostId>("\"Dookie\"").is_err());
}

#[test]
fn topology_material_changes_pool_identity() {
    let host = HostId::new("dookie").unwrap();
    let first = PoolKey::new(
        host.clone(),
        TopologyRevision::from_material(b"ssh:dookie:22:user-a"),
    );
    let second = PoolKey::new(
        host,
        TopologyRevision::from_material(b"ssh:dookie:22:user-b"),
    );
    assert_ne!(first, second);
}
