use super::path::VirtualPath;
use super::workspace::StateWorkspace;

#[tokio::test]
async fn list_omits_reserved_metadata_paths() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = StateWorkspace::new(temp.path());
    let path = VirtualPath::parse("visible.txt").unwrap();
    workspace.write_file(&path, "hello").await.unwrap();
    tokio::fs::create_dir_all(temp.path().join(".soma-state"))
        .await
        .unwrap();

    let entries = workspace
        .list(&VirtualPath::parse_read_scope("").unwrap())
        .await
        .unwrap();

    assert_eq!(entries.entries, vec!["visible.txt"]);
}
