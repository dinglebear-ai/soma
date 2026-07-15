use super::workspace_archive::WorkspaceArchiveMeta;

#[test]
fn archive_meta_serializes() {
    let value = serde_json::to_value(WorkspaceArchiveMeta { files: 1, bytes: 2 }).unwrap();
    assert_eq!(value["files"], 1);
}
