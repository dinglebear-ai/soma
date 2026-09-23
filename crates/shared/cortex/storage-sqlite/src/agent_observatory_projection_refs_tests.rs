use super::*;
use crate::{StorageConfig, init_pool};

#[test]
fn missing_worktree_reference_returns_actionable_error() {
    let dir = tempfile::tempdir().unwrap();
    let pool = init_pool(&StorageConfig {
        db_path: dir.path().join("refs.db"),
        pool_size: 1,
        wal_mode: false,
        ..StorageConfig::default()
    })
    .unwrap();
    let mut conn = pool.get().unwrap();
    let tx = conn.transaction().unwrap();
    let error = worktree_id(&tx, "missing-worktree")
        .unwrap_err()
        .to_string();
    assert!(error.contains("worktree not found for key missing-worktree"));
}
