use super::*;

#[test]
fn prune_counts_are_compact_and_complete() {
    let fingerprint = soma_infra::DockerPruneFingerprint {
        host: soma_fleet::HostId::new("devhost").unwrap(),
        topology_revision: soma_fleet::HostRecord::new(
            soma_fleet::HostId::new("devhost").unwrap(),
            soma_fleet::HostEndpoint::Local,
        )
        .revision()
        .clone(),
        target: soma_infra::DockerPruneTarget::All,
        containers: vec!["a".into()],
        images: vec!["b".into()],
        volumes: vec!["c".into()],
        networks: vec!["d".into()],
        build_cache_bytes: 9,
        sha256: "e".repeat(64),
    };
    let counts = prune_counts(&fingerprint);
    assert_eq!(counts["containers"], 1);
    assert_eq!(counts["build_cache_bytes"], 9);
}
