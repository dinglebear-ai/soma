use super::artifacts::ArtifactStore;

#[test]
fn artifact_store_root_uses_run_id() {
    assert!(ArtifactStore::new("abc").unwrap().root().ends_with("abc"));
}
