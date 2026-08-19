use std::{collections::HashSet, time::Duration};

use super::prune::{
    ActiveArtifactRun, active_artifact_runs_snapshot, prune_artifact_runs_in, prune_old_runs,
};

async fn create_run(root: &std::path::Path, name: &str, bytes: usize) {
    let path = root.join(name);
    tokio::fs::create_dir_all(&path).await.unwrap();
    tokio::fs::write(path.join("payload.bin"), vec![b'x'; bytes])
        .await
        .unwrap();
}

#[tokio::test]
async fn prune_noops_when_root_missing() {
    let root = tempfile::tempdir().unwrap().path().join("missing");
    assert_eq!(prune_old_runs(&root, 2).await.unwrap(), 0);
}

#[tokio::test]
async fn retention_keeps_newest_runs_by_modification_time() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    create_run(root, "old-run", 1).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    create_run(root, "middle-run", 1).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    create_run(root, "new-run", 1).await;

    assert_eq!(prune_artifact_runs_in(root, 2, 0, &HashSet::new()).await, 1);
    assert!(!root.join("old-run").exists());
    assert!(root.join("middle-run").exists());
    assert!(root.join("new-run").exists());
}

#[tokio::test]
async fn byte_budget_prunes_oldest_runs_even_when_count_pruning_is_disabled() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    create_run(root, "old-run", 5).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    create_run(root, "new-run", 5).await;

    assert_eq!(prune_artifact_runs_in(root, 0, 5, &HashSet::new()).await, 1);
    assert!(!root.join("old-run").exists());
    assert!(root.join("new-run").exists());
}

#[tokio::test]
async fn active_runs_are_never_pruned() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    create_run(root, "active-old", 8).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    create_run(root, "new-run", 8).await;
    let active = HashSet::from(["active-old".to_owned()]);

    assert_eq!(prune_artifact_runs_in(root, 1, 8, &active).await, 0);
    assert!(root.join("active-old").exists());
    assert!(root.join("new-run").exists());
}

#[tokio::test]
async fn unsafe_or_non_directory_entries_are_not_collected() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    create_run(root, "managed-run", 1).await;
    create_run(root, "operator notes", 1).await;
    tokio::fs::write(root.join("plain-file"), b"keep me")
        .await
        .unwrap();

    assert_eq!(prune_artifact_runs_in(root, 0, 1, &HashSet::new()).await, 0);
    assert!(root.join("operator notes").exists());
    assert!(root.join("plain-file").exists());
}

#[test]
fn active_run_guard_registers_and_unregisters_with_duplicate_ids() {
    assert!(!active_artifact_runs_snapshot().contains("guard-test"));
    let first = ActiveArtifactRun::register("guard-test");
    let second = ActiveArtifactRun::register("guard-test");
    assert!(active_artifact_runs_snapshot().contains("guard-test"));
    drop(first);
    assert!(active_artifact_runs_snapshot().contains("guard-test"));
    drop(second);
    assert!(!active_artifact_runs_snapshot().contains("guard-test"));
}
