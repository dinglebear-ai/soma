use super::store::ArtifactStore;

#[tokio::test]
async fn artifact_store_writes_receipt() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_var("SOMA_HOME", temp.path());
    let receipt = ArtifactStore::new("run")
        .unwrap()
        .write_text("out.txt", "hello", None)
        .await
        .unwrap();
    std::env::remove_var("SOMA_HOME");
    assert_eq!(receipt.bytes, 5);
    assert_eq!(receipt.content_type, "text/plain");
}

#[test]
fn artifact_store_rejects_unsafe_run_ids() {
    assert!(ArtifactStore::new("../escape").is_err());
    assert!(ArtifactStore::new("/tmp/escape").is_err());
    assert!(ArtifactStore::new("safe-run_01").is_ok());
}

#[tokio::test]
async fn artifact_store_enforces_run_quota() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_var("SOMA_HOME", temp.path());
    let store = ArtifactStore::new("run").unwrap().with_run_limits(5, 1);

    store.write_text("a.txt", "hello", None).await.unwrap();
    let err = store.write_text("b.txt", "x", None).await.unwrap_err();

    std::env::remove_var("SOMA_HOME");
    assert_eq!(err.kind(), "invalid_param");
}
