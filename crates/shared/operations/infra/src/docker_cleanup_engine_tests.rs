use super::*;
use soma_fleet::{HostEndpoint, HostId, HostRecord};
use std::collections::BTreeMap;

fn image(id: &str, tags: &[&str], digests: &[&str]) -> ImageSummary {
    let host = HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local);
    ImageSummary {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        id: id.into(),
        repo_tags: tags.iter().map(|value| (*value).into()).collect(),
        repo_digests: digests.iter().map(|value| (*value).into()).collect(),
        created_unix_seconds: 0,
        size_bytes: 0,
        containers: 0,
        labels: BTreeMap::new(),
    }
}

#[test]
fn image_lookup_matches_ids_tags_and_digests() {
    let images = [image("sha256:a", &["api:v1"], &["api@sha256:b"])];
    assert_eq!(find_image(&images, "sha256:a").unwrap().id, "sha256:a");
    assert_eq!(find_image(&images, "api:v1").unwrap().id, "sha256:a");
    assert_eq!(find_image(&images, "api@sha256:b").unwrap().id, "sha256:a");
    assert!(find_image(&images, "missing").is_none());
}

#[test]
fn prune_verification_rejects_reported_identity_that_remains() {
    let host = HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local);
    let after = DockerPruneFingerprint {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        target: DockerPruneTarget::Images,
        containers: Vec::new(),
        images: vec!["sha256:a".into()],
        volumes: Vec::new(),
        networks: Vec::new(),
        build_cache_bytes: 0,
        sha256: String::new(),
    }
    .finalize()
    .unwrap();
    let receipt = crate::DockerPruneReceipt {
        send_state: MutationSendState::Sent,
        scopes: vec![crate::DockerPruneScopeReceipt {
            target: DockerPruneTarget::Images,
            deleted: vec!["sha256:a".into()],
            space_reclaimed: 1,
        }],
    };
    assert!(verify_prune(&receipt, &after, &after).is_err());
}
