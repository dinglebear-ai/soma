use super::path::VirtualPath;
use super::workspace::{FileEdit, StateWorkspace};

#[tokio::test]
async fn edit_plan_applies_normalized_file_edits() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = StateWorkspace::new(temp.path());
    let path = VirtualPath::parse("notes/a.txt").unwrap();
    workspace.write_file(&path, "hello lab").await.unwrap();

    let plan = workspace
        .plan_edits(vec![FileEdit {
            path: "notes/a.txt".to_string(),
            search: "lab".to_string(),
            replace: "soma".to_string(),
        }])
        .await
        .unwrap();
    let applied = workspace.apply_edit_plan(&plan.plan_id).await.unwrap();

    assert!(applied.ok);
    assert_eq!(applied.changed, vec!["notes/a.txt"]);
    assert_eq!(
        workspace.read_file(&path).await.unwrap().content,
        "hello soma"
    );
}
