use super::*;

#[test]
fn replacement_evidence_uris_preserve_target_identity() {
    assert_eq!(
        container_diff_uri("devhost", "old-id", "new-id"),
        "container-recreate://devhost/old-id/new-id"
    );
    assert_eq!(
        compose_diff_uri("devhost", "soma"),
        "compose-recreate://devhost/soma"
    );
}
