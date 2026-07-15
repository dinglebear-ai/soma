use super::store::ArtifactStore;

#[tokio::test]
async fn artifact_store_writes_receipt() {
    let temp = tempfile::tempdir().unwrap();
    std::env::set_var("SOMA_HOME", temp.path());
    let receipt = ArtifactStore::new("run")
        .write_text("out.txt", "hello", None)
        .await
        .unwrap();
    std::env::remove_var("SOMA_HOME");
    assert_eq!(receipt.bytes, 5);
    assert_eq!(receipt.content_type, "text/plain");
}
