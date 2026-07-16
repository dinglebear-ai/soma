use super::path::VirtualPath;
use super::workspace::StateWorkspace;

#[tokio::test]
async fn workspace_writes_and_reads_file() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = StateWorkspace::new(temp.path());
    let path = VirtualPath::parse("notes/a.txt").unwrap();
    workspace.write_file(&path, "hello").await.unwrap();
    assert_eq!(workspace.read_file(&path).await.unwrap().content, "hello");
}
