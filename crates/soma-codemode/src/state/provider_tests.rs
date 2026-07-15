use serde_json::json;

use super::provider::StateProvider;
use super::workspace::StateWorkspace;

#[tokio::test]
async fn provider_dispatches_write_and_read() {
    let temp = tempfile::tempdir().unwrap();
    let provider = StateProvider::new(StateWorkspace::new(temp.path()));
    provider
        .dispatch("write_file", json!({"path": "a.txt", "content": "hello"}))
        .await
        .unwrap();
    let read = provider
        .dispatch("read_file", json!({"path": "a.txt"}))
        .await
        .unwrap();
    assert_eq!(read["content"], "hello");
}
