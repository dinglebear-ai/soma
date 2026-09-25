use super::*;

#[test]
fn stale_topology_errors_preserve_both_revisions() {
    let error = FleetError::StaleTopology {
        host: HostId::new("devhost").unwrap(),
        expected: TopologyRevision::from_material(b"old"),
        actual: TopologyRevision::from_material(b"new"),
    };
    let text = error.to_string();
    assert!(text.contains("devhost"));
    assert!(text.contains("stale topology"));
}

#[test]
fn driver_errors_keep_target_identity() {
    let error = FleetError::Connection {
        host: HostId::new("devhost").unwrap(),
        message: "strict known-host verification failed".into(),
    };
    assert_eq!(
        error.to_string(),
        "connection to devhost failed: strict known-host verification failed"
    );
}
