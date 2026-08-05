use super::*;
use soma_fleet::{HostEndpoint, HostId, HostRecord};

#[test]
fn prune_targets_are_closed_and_expand_deterministically() {
    assert_eq!(
        DockerPruneTarget::parse("buildcache").unwrap(),
        DockerPruneTarget::BuildCache
    );
    assert!(DockerPruneTarget::parse("everything").is_err());
    assert_eq!(DockerPruneTarget::All.expanded().len(), 5);
}

#[test]
fn image_fingerprint_is_order_independent() {
    let left = ImageRemovalFingerprint::new(
        "api:v1",
        ImageIdentity {
            id: "sha256:a".into(),
            repo_tags: vec!["api:v1".into(), "api:latest".into()],
            repo_digests: vec!["api@sha256:b".into()],
        },
    )
    .unwrap();
    let right = ImageRemovalFingerprint::new(
        "api:v1",
        ImageIdentity {
            id: "sha256:a".into(),
            repo_tags: vec!["api:latest".into(), "api:v1".into()],
            repo_digests: vec!["api@sha256:b".into()],
        },
    )
    .unwrap();
    assert_eq!(left.sha256, right.sha256);
}

#[test]
fn prune_fingerprint_sorts_candidate_sets() {
    let host = HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local);
    let fp = DockerPruneFingerprint {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        target: DockerPruneTarget::Images,
        containers: vec!["b".into(), "a".into()],
        images: vec!["z".into(), "z".into()],
        volumes: Vec::new(),
        networks: Vec::new(),
        build_cache_bytes: 0,
        sha256: String::new(),
    }
    .finalize()
    .unwrap();
    assert_eq!(fp.containers, ["a", "b"]);
    assert_eq!(fp.images, ["z"]);
    assert_eq!(fp.sha256.len(), 64);
}

#[test]
fn cleanup_fingerprints_use_sha2_011_compatible_lowercase_hex() {
    let identity = ImageIdentity {
        id: "sha256:image".into(),
        repo_tags: vec!["app:v1".into()],
        repo_digests: vec!["app@sha256:digest".into()],
    };
    let fingerprint = ImageRemovalFingerprint::new("app:v1", identity).unwrap();
    assert_eq!(fingerprint.sha256.len(), 64);
    assert!(fingerprint.sha256.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')));
}
