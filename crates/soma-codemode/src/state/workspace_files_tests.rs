use super::path::VirtualPath;
use super::workspace::StateWorkspace;

#[tokio::test]
async fn read_file_reports_content_and_size() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = StateWorkspace::new(temp.path());
    let path = VirtualPath::parse("notes/a.txt").unwrap();

    workspace.write_file(&path, "hello").await.unwrap();
    let read = workspace.read_file(&path).await.unwrap();

    assert_eq!(read.path, "notes/a.txt");
    assert_eq!(read.content, "hello");
    assert_eq!(read.bytes, 5);
}
