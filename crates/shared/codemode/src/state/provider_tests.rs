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

#[tokio::test]
async fn provider_exposes_ported_workspace_methods() {
    let temp = tempfile::tempdir().unwrap();
    let provider = StateProvider::new(StateWorkspace::new(temp.path()));
    provider
        .dispatch(
            "write_json",
            json!({"path": "data.json", "value": {"name": "soma"}}),
        )
        .await
        .unwrap();
    provider
        .dispatch(
            "append_file",
            json!({"path": "notes.txt", "content": "hello"}),
        )
        .await
        .unwrap();

    assert_eq!(
        provider
            .dispatch("read_json", json!({"path": "data.json"}))
            .await
            .unwrap()["value"]["name"],
        "soma"
    );
    assert_eq!(
        provider
            .dispatch("list", json!({"path": ""}))
            .await
            .unwrap()["entries"][0],
        "data.json"
    );
    assert_eq!(
        provider
            .dispatch("hash_file", json!({"path": "notes.txt"}))
            .await
            .unwrap()["algorithm"],
        "sha256"
    );
}
