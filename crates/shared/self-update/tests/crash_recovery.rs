#![cfg(unix)]

use serde_json::json;
use sha2::{Digest, Sha256};
use soma_self_update::{RecoveryAction, UpdateLayout, UpdatePolicy, Updater};
use tempfile::tempdir;

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn write_marker(
    state: &std::path::Path,
    executable: &std::path::Path,
    backup: &std::path::Path,
    staged: &std::path::Path,
    phase: &str,
    old: &[u8],
    new: &[u8],
) {
    std::fs::write(
        state,
        serde_json::to_vec_pretty(&json!({
            "schema_version": 2,
            "phase": phase,
            "target": "2.0.0",
            "previous": "1.0.0",
            "executable": executable,
            "backup": backup,
            "staged": staged,
            "attempts": 0,
            "sha256": digest(new),
            "previous_sha256": digest(old),
        }))
        .unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn prepared_marker_with_previous_binary_aborts_idempotently() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("agent");
    let state = temp.path().join("update.json");
    let backup = temp.path().join(".agent.rollback-999999-1");
    let staged = temp.path().join(".agent.update-999999-2.part");
    let old = b"old executable";
    let new = b"new executable";
    std::fs::write(&executable, old).unwrap();
    std::fs::write(&backup, old).unwrap();
    std::fs::write(&staged, new).unwrap();
    write_marker(&state, &executable, &backup, &staged, "prepared", old, new);
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );

    assert_eq!(
        updater.recover_on_startup("1.0.0").await.unwrap(),
        RecoveryAction::NoPendingUpdate
    );
    assert_eq!(std::fs::read(&executable).unwrap(), old);
    assert!(!state.exists());
    assert!(!backup.exists());
    assert!(!staged.exists());
    assert_eq!(
        updater.recover_on_startup("1.0.0").await.unwrap(),
        RecoveryAction::NoPendingUpdate
    );
}

#[tokio::test]
async fn prepared_marker_with_target_binary_completes_the_install_phase() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("agent");
    let state = temp.path().join("update.json");
    let backup = temp.path().join(".agent.rollback-999999-1");
    let staged = temp.path().join(".agent.update-999999-2.part");
    let old = b"old executable";
    let new = b"new executable";
    std::fs::write(&executable, new).unwrap();
    std::fs::write(&backup, old).unwrap();
    write_marker(&state, &executable, &backup, &staged, "prepared", old, new);
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );

    assert!(matches!(
        updater.recover_on_startup("2.0.0").await.unwrap(),
        RecoveryAction::PendingUpdate { attempts: 1, .. }
    ));
    let marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state).unwrap()).unwrap();
    assert_eq!(marker["phase"], "installed");
    assert!(backup.exists());
}

#[tokio::test]
async fn installed_marker_rejects_changed_executable_before_counting_attempt() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("agent");
    let state = temp.path().join("update.json");
    let backup = temp.path().join(".agent.rollback-999999-1");
    let staged = temp.path().join(".agent.update-999999-2.part");
    let old = b"old executable";
    let new = b"new executable";
    let changed = b"changed executable reporting 2.0.0";
    std::fs::write(&executable, changed).unwrap();
    std::fs::write(&backup, old).unwrap();
    write_marker(&state, &executable, &backup, &staged, "installed", old, new);
    let original_marker = std::fs::read(&state).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );

    assert!(matches!(
        updater.recover_on_startup("2.0.0").await,
        Err(soma_self_update::UpdateError::DigestMismatch { .. })
    ));
    assert_eq!(std::fs::read(&state).unwrap(), original_marker);
    assert_eq!(std::fs::read(&backup).unwrap(), old);
    assert_eq!(std::fs::read(&executable).unwrap(), changed);
}

#[tokio::test]
async fn rolling_back_marker_with_previous_binary_finishes_idempotently() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("agent");
    let state = temp.path().join("update.json");
    let backup = temp.path().join(".agent.rollback-999999-1");
    let staged = temp.path().join(".agent.update-999999-2.part");
    let old = b"old executable";
    let new = b"new executable";
    std::fs::write(&executable, old).unwrap();
    write_marker(
        &state,
        &executable,
        &backup,
        &staged,
        "rolling_back",
        old,
        new,
    );
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );

    assert!(matches!(
        updater.recover_on_startup("1.0.0").await.unwrap(),
        RecoveryAction::RollbackInstalled { .. }
    ));
    assert_eq!(std::fs::read(&executable).unwrap(), old);
    assert!(!state.exists());
    assert_eq!(
        updater.recover_on_startup("1.0.0").await.unwrap(),
        RecoveryAction::NoPendingUpdate
    );
}

#[tokio::test]
async fn rolled_back_marker_is_finalized_without_repeating_the_rename() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("agent");
    let state = temp.path().join("update.json");
    let backup = temp.path().join(".agent.rollback-999999-1");
    let staged = temp.path().join(".agent.update-999999-2.part");
    let old = b"old executable";
    let new = b"new executable";
    std::fs::write(&executable, old).unwrap();
    write_marker(
        &state,
        &executable,
        &backup,
        &staged,
        "rolled_back",
        old,
        new,
    );
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );

    assert!(matches!(
        updater.recover_on_startup("1.0.0").await.unwrap(),
        RecoveryAction::RollbackInstalled { .. }
    ));
    assert!(!state.exists());
}

#[tokio::test]
async fn marker_rejects_loose_backup_names_and_symlink_backups() {
    for attack in ["loose_name", "symlink"] {
        let temp = tempdir().unwrap();
        let executable = temp.path().join("agent");
        let state = temp.path().join("update.json");
        let exact = temp.path().join(".agent.rollback-999999-1");
        let backup = if attack == "loose_name" {
            temp.path().join(".agent.rollback-999999-1-extra")
        } else {
            exact
        };
        let staged = temp.path().join(".agent.update-999999-2.part");
        let old = b"old executable";
        let new = b"new executable";
        std::fs::write(&executable, new).unwrap();
        if attack == "symlink" {
            let target = temp.path().join("attacker-controlled");
            std::fs::write(&target, old).unwrap();
            std::os::unix::fs::symlink(&target, &backup).unwrap();
        } else {
            std::fs::write(&backup, old).unwrap();
        }
        write_marker(&state, &executable, &backup, &staged, "installed", old, new);
        let updater = Updater::new(
            UpdateLayout::new(&executable, &state),
            UpdatePolicy::default(),
        );

        assert!(matches!(
            updater.recover_on_startup("2.0.0").await,
            Err(soma_self_update::UpdateError::InvalidMarker { .. })
        ));
        assert_eq!(std::fs::read(&executable).unwrap(), new);
        assert!(state.exists());
    }
}

#[tokio::test]
async fn rollback_rejects_backup_bytes_that_do_not_match_previous_digest() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("agent");
    let state = temp.path().join("update.json");
    let backup = temp.path().join(".agent.rollback-999999-1");
    let staged = temp.path().join(".agent.update-999999-2.part");
    let old = b"old executable";
    let new = b"new executable";
    std::fs::write(&executable, new).unwrap();
    std::fs::write(&backup, b"different bytes").unwrap();
    write_marker(&state, &executable, &backup, &staged, "installed", old, new);
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default()
            .with_max_unconfirmed_restarts(1)
            .unwrap(),
    );

    assert!(matches!(
        updater.recover_on_startup("2.0.0").await.unwrap(),
        RecoveryAction::PendingUpdate { attempts: 1, .. }
    ));
    assert!(matches!(
        updater.recover_on_startup("2.0.0").await,
        Err(soma_self_update::UpdateError::InvalidMarker { .. })
    ));
    assert_eq!(std::fs::read(&executable).unwrap(), new);
    assert!(state.exists());
}

#[tokio::test]
async fn prepared_marker_rejects_an_unowned_staged_path_without_deleting_it() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("agent");
    let state = temp.path().join("update.json");
    let backup = temp.path().join(".agent.rollback-999999-1");
    let unrelated = temp.path().join("unrelated.txt");
    let old = b"old executable";
    let new = b"new executable";
    std::fs::write(&executable, old).unwrap();
    std::fs::write(&backup, old).unwrap();
    std::fs::write(&unrelated, b"must survive").unwrap();
    write_marker(
        &state,
        &executable,
        &backup,
        &unrelated,
        "prepared",
        old,
        new,
    );
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );

    assert!(matches!(
        updater.recover_on_startup("1.0.0").await,
        Err(soma_self_update::UpdateError::InvalidMarker { .. })
    ));
    assert_eq!(std::fs::read(&unrelated).unwrap(), b"must survive");
    assert!(state.exists());
}
