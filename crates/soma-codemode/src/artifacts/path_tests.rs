use super::path::{artifact_root, safe_artifact_path};

#[test]
fn artifact_paths_stay_under_run_root() {
    let root = artifact_root("run");
    assert!(root.ends_with("code-mode-artifacts/run"));
    assert!(safe_artifact_path(&root, "nested/out.txt").is_ok());
    assert!(safe_artifact_path(&root, "../out.txt").is_err());
}
