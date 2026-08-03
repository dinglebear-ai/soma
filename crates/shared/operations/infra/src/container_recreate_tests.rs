use super::*;

#[test]
fn recreate_fingerprints_require_complete_identity_and_sha256() {
    assert!(
        ContainerRecreateFingerprint::new(
            "abc",
            "app",
            "app:v1",
            ContainerState::Running,
            "a".repeat(64)
        )
        .is_ok()
    );
    assert!(
        ContainerRecreateFingerprint::new(
            "abc",
            "",
            "app:v1",
            ContainerState::Running,
            "a".repeat(64)
        )
        .is_err()
    );
    assert!(
        ContainerRecreateFingerprint::new("abc", "app", "app:v1", ContainerState::Running, "bad")
            .is_err()
    );
}
