#![cfg(unix)]

use std::fs::OpenOptions;
use std::os::unix::fs::PermissionsExt;

use fs2::FileExt;
use sha2::{Digest, Sha256};
use soma_self_update::{
    BackupStrategy, ConfirmationOutcome, InstallOutcome, RecoveryAction, UpdateDirective,
    UpdateError, UpdateLayout, UpdatePolicy, Updater,
};
use tempfile::tempdir;

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[tokio::test]
async fn install_rehashes_validated_bytes_before_mutating_live_state() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let artifact = validated(&updater, new, "2.0.0").await;
    std::fs::write(artifact.path(), b"mutated after validation").unwrap();
    assert!(matches!(
        updater.install(artifact, "1.0.0").await,
        Err(UpdateError::DigestMismatch { .. })
    ));
    assert_eq!(std::fs::read(&executable).unwrap(), old);
    assert!(!state.exists());
}

#[tokio::test]
async fn validation_rejects_a_candidate_that_changes_its_private_mode() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\nchmod 0644 \"$0\"\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o750)).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let directive = UpdateDirective::new("2.0.0", "/binary", digest(new)).unwrap();
    let staged = updater.stage(&new[..], &directive).await.unwrap();
    assert!(matches!(
        updater.validate(staged).await,
        Err(UpdateError::InvalidStagedArtifact { .. })
    ));
    assert_eq!(std::fs::read(&executable).unwrap(), old);
    assert!(!state.exists());
}

#[tokio::test]
async fn supported_source_mode_is_applied_only_during_final_install() {
    for intended_mode in [0o700, 0o750, 0o755] {
        let temp = tempdir().unwrap();
        let executable = temp.path().join("example");
        let state = temp.path().join("update.json");
        let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
        let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
        std::fs::write(&executable, old).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(intended_mode))
            .unwrap();
        let updater = Updater::new(
            UpdateLayout::new(&executable, &state),
            UpdatePolicy::default(),
        );
        let directive = UpdateDirective::new("2.0.0", "/binary", digest(new)).unwrap();
        let staged = updater.stage(&new[..], &directive).await.unwrap();
        assert_eq!(
            std::fs::metadata(staged.path())
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o700
        );
        let validated = updater.validate(staged).await.unwrap();
        assert_eq!(
            std::fs::metadata(validated.path())
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o700
        );

        updater.install(validated, "1.0.0").await.unwrap();

        assert_eq!(
            std::fs::metadata(&executable).unwrap().permissions().mode() & 0o7777,
            intended_mode
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn install_yields_the_async_executor_while_transaction_work_blocks() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let artifact = validated(&updater, new, "2.0.0").await;
    let progressed = Arc::new(AtomicBool::new(false));
    let task_progressed = Arc::clone(&progressed);
    let unrelated_task = tokio::spawn(async move {
        task_progressed.store(true, Ordering::SeqCst);
    });

    updater.install(artifact, "1.0.0").await.unwrap();

    assert!(progressed.load(Ordering::SeqCst));
    unrelated_task.await.unwrap();
}

#[tokio::test]
async fn oversized_previous_version_is_rejected_before_backup_or_swap() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let artifact = validated(&updater, new, "2.0.0").await;

    assert!(matches!(
        updater.install(artifact, "1".repeat(70 * 1024)).await,
        Err(UpdateError::InvalidMarker { .. })
    ));
    assert_eq!(std::fs::read(&executable).unwrap(), old);
    assert!(!state.exists());
    assert!(
        std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().contains(".rollback-"))
    );
}

#[tokio::test]
async fn install_rejects_a_validated_path_replaced_by_a_symlink() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let other = temp.path().join("other");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    std::fs::write(&other, new).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let artifact = validated(&updater, new, "2.0.0").await;
    let staged_path = artifact.path().to_path_buf();
    std::fs::remove_file(&staged_path).unwrap();
    std::os::unix::fs::symlink(&other, &staged_path).unwrap();
    assert!(matches!(
        updater.install(artifact, "1.0.0").await,
        Err(UpdateError::InvalidStagedArtifact { .. })
    ));
    assert_eq!(std::fs::read(&executable).unwrap(), old);
    assert!(!state.exists());
}

#[tokio::test]
async fn install_rejects_same_bytes_from_a_replaced_regular_inode() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let artifact = validated(&updater, new, "2.0.0").await;
    let staged_path = artifact.path().to_path_buf();
    std::fs::remove_file(&staged_path).unwrap();
    std::fs::write(&staged_path, new).unwrap();

    assert!(
        updater.install(artifact, "1.0.0").await.is_err(),
        "a replacement inode with identical bytes was installed"
    );
    assert_eq!(std::fs::read(&executable).unwrap(), old);
    assert!(!state.exists());
}

#[tokio::test]
async fn copy_backup_and_rollback_preserve_only_safe_unix_modes() {
    use std::os::unix::fs::PermissionsExt;

    for mode in [0o700, 0o750, 0o755] {
        let temp = tempdir().unwrap();
        let executable = temp.path().join("example");
        let state = temp.path().join("update.json");
        let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
        let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
        std::fs::write(&executable, old).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(mode)).unwrap();
        let policy = UpdatePolicy::default()
            .with_backup_strategy(BackupStrategy::Copy)
            .with_max_unconfirmed_restarts(1)
            .unwrap();
        let updater = Updater::new(UpdateLayout::new(&executable, &state), policy);
        updater
            .install(validated(&updater, new, "2.0.0").await, "1.0.0")
            .await
            .unwrap();
        let marker: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&state).unwrap()).unwrap();
        let backup = std::path::Path::new(marker["backup"].as_str().unwrap());
        assert_eq!(
            std::fs::metadata(backup).unwrap().permissions().mode() & 0o7777,
            mode
        );
        assert_eq!(
            std::fs::metadata(backup).unwrap().permissions().mode() & 0o7022,
            0
        );
        updater.recover_on_startup("2.0.0").await.unwrap();
        updater.recover_on_startup("2.0.0").await.unwrap();
        assert_eq!(
            std::fs::metadata(&executable).unwrap().permissions().mode() & 0o7777,
            mode
        );
    }
}

#[tokio::test]
async fn backup_directory_must_sync_before_marker_is_persisted() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let binary_dir = temp.path().join("bin");
    let state_dir = temp.path().join("state");
    std::fs::create_dir(&binary_dir).unwrap();
    std::fs::create_dir(&state_dir).unwrap();
    let executable = binary_dir.join("example");
    let state = state_dir.join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let artifact = validated(&updater, new, "2.0.0").await;
    std::fs::set_permissions(&binary_dir, std::fs::Permissions::from_mode(0o300)).unwrap();
    let result = updater.install(artifact, "1.0.0").await;
    std::fs::set_permissions(&binary_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
    assert!(
        !state.exists(),
        "marker must not outlive a failed backup fsync"
    );
    assert_eq!(std::fs::read(&executable).unwrap(), old);
    assert_eq!(
        std::fs::read_dir(&binary_dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("rollback"))
            .count(),
        0
    );
}

#[tokio::test]
async fn startup_reclaims_only_owned_crash_leftovers() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    std::fs::write(&executable, b"old").unwrap();
    let mut exited = std::process::Command::new("sh")
        .arg("-c")
        .arg("exit 0")
        .spawn()
        .unwrap();
    let dead_pid = exited.id();
    exited.wait().unwrap();
    let owned_stage = temp
        .path()
        .join(format!(".example.update-{dead_pid}-1.part"));
    let owned_backup = temp.path().join(format!(".example.rollback-{dead_pid}-1"));
    let unrelated = temp.path().join(".other.update-123-1.part");
    let matching_directory = temp.path().join(".example.rollback-directory");
    let loose_backup = temp
        .path()
        .join(format!(".example.rollback-{dead_pid}-1-extra"));
    std::fs::write(&owned_stage, b"leftover").unwrap();
    std::fs::write(&owned_backup, b"leftover").unwrap();
    std::fs::write(&unrelated, b"keep").unwrap();
    std::fs::create_dir(&matching_directory).unwrap();
    std::fs::write(&loose_backup, b"keep").unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    assert_eq!(
        updater.recover_on_startup("1").await.unwrap(),
        RecoveryAction::NoPendingUpdate
    );
    assert!(!owned_stage.exists());
    assert!(!owned_backup.exists());
    assert!(unrelated.exists());
    assert!(matching_directory.exists());
    assert!(loose_backup.exists());
}

#[tokio::test]
async fn install_and_recovery_do_not_delete_live_concurrent_stages() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let install = validated(&updater, new, "2.0.0").await;
    let other = validated(&updater, new, "2.0.0").await;
    let other_path = other.path().to_path_buf();

    updater.install(install, "1.0.0").await.unwrap();
    assert!(
        other_path.exists(),
        "install deleted a concurrent live stage"
    );
    updater.recover_on_startup("2.0.0").await.unwrap();
    assert!(
        other_path.exists(),
        "recovery deleted a concurrent live stage"
    );
    drop(other);
}

#[tokio::test]
async fn symlinked_executable_parent_does_not_hide_the_protected_stage() {
    let temp = tempdir().unwrap();
    let real = temp.path().join("real");
    let alias = temp.path().join("alias");
    std::fs::create_dir(&real).unwrap();
    std::os::unix::fs::symlink(&real, &alias).unwrap();
    let executable = alias.join("example");
    let state = temp.path().join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let artifact = validated(&updater, new, "2.0.0").await;

    updater.install(artifact, "1.0.0").await.unwrap();
    assert_eq!(std::fs::read(real.join("example")).unwrap(), new);
}

#[tokio::test]
async fn install_rejects_artifact_staged_by_another_layout_before_lock_creation() {
    let temp = tempdir().unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    let first_executable = first.join("example");
    let second_executable = second.join("example");
    let first_state = first.join("update.json");
    let second_state = second.join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&first_executable, old).unwrap();
    std::fs::write(&second_executable, old).unwrap();
    let first_updater = Updater::new(
        UpdateLayout::new(&first_executable, &first_state),
        UpdatePolicy::default(),
    );
    let second_updater = Updater::new(
        UpdateLayout::new(&second_executable, &second_state),
        UpdatePolicy::default(),
    );
    let artifact = validated(&first_updater, new, "2.0.0").await;

    assert!(matches!(
        second_updater.install(artifact, "1.0.0").await,
        Err(UpdateError::InvalidStagedArtifact { .. })
    ));
    assert_eq!(std::fs::read(&second_executable).unwrap(), old);
    assert!(!second_state.exists());
    assert!(!second_state.with_extension("json.lock").exists());
    assert!(!second.join(".example.update.lock").exists());
}

#[tokio::test]
async fn install_rejects_sibling_executables_staged_artifact_before_lock_creation() {
    let temp = tempdir().unwrap();
    let foo_executable = temp.path().join("foo");
    let bar_executable = temp.path().join("bar");
    let foo_state = temp.path().join("foo-state.json");
    let bar_state = temp.path().join("bar-state.json");
    let foo_old = b"#!/bin/sh\necho 'foo 1.0.0'\n";
    let bar_old = b"#!/bin/sh\necho 'bar 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'foo 2.0.0'\n";
    std::fs::write(&foo_executable, foo_old).unwrap();
    std::fs::write(&bar_executable, bar_old).unwrap();
    let foo_updater = Updater::new(
        UpdateLayout::new(&foo_executable, &foo_state),
        UpdatePolicy::default(),
    );
    let bar_updater = Updater::new(
        UpdateLayout::new(&bar_executable, &bar_state),
        UpdatePolicy::default(),
    );
    let artifact = validated(&foo_updater, new, "2.0.0").await;
    let staged = artifact.path().to_path_buf();

    assert!(matches!(
        bar_updater.install(artifact, "1.0.0").await,
        Err(UpdateError::InvalidStagedArtifact { .. })
    ));
    assert_eq!(std::fs::read(&foo_executable).unwrap(), foo_old);
    assert_eq!(std::fs::read(&bar_executable).unwrap(), bar_old);
    assert!(!foo_state.exists());
    assert!(!bar_state.exists());
    assert!(!foo_state.with_extension("json.lock").exists());
    assert!(!bar_state.with_extension("json.lock").exists());
    assert!(!temp.path().join(".foo.update.lock").exists());
    assert!(!temp.path().join(".bar.update.lock").exists());
    assert!(!staged.exists());
}

#[tokio::test]
async fn install_rejects_stage_after_executable_parent_symlink_retarget() {
    let temp = tempdir().unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    let alias = temp.path().join("current");
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    std::os::unix::fs::symlink(&first, &alias).unwrap();
    let executable = alias.join("example");
    let state = temp.path().join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(first.join("example"), old).unwrap();
    std::fs::write(second.join("example"), old).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let artifact = validated(&updater, new, "2.0.0").await;
    std::fs::remove_file(&alias).unwrap();
    std::os::unix::fs::symlink(&second, &alias).unwrap();

    assert!(matches!(
        updater.install(artifact, "1.0.0").await,
        Err(UpdateError::InvalidStagedArtifact { .. })
    ));
    assert_eq!(std::fs::read(second.join("example")).unwrap(), old);
    assert!(!state.exists());
    assert!(!state.with_extension("json.lock").exists());
    assert!(!second.join(".example.update.lock").exists());
}

#[tokio::test]
async fn install_rejects_executable_identity_change_after_staging() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let replacement = temp.path().join("replacement");
    let state = temp.path().join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let other = b"#!/bin/sh\necho 'example 1.0.1'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let artifact = validated(&updater, new, "2.0.0").await;
    std::fs::write(&replacement, other).unwrap();
    std::fs::set_permissions(&replacement, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::rename(&replacement, &executable).unwrap();

    assert!(matches!(
        updater.install(artifact, "1.0.0").await,
        Err(UpdateError::ExecutableIdentityChanged { .. })
    ));
    assert_eq!(std::fs::read(&executable).unwrap(), other);
    assert!(!state.exists());
}

#[tokio::test]
async fn install_rejects_executable_mode_change_after_staging() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let artifact = validated(&updater, new, "2.0.0").await;
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert!(matches!(
        updater.install(artifact, "1.0.0").await,
        Err(UpdateError::ExecutableIdentityChanged { .. })
    ));
    assert_eq!(
        std::fs::metadata(&executable).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert!(!state.exists());
}

#[tokio::test]
async fn oversized_markers_fail_bounded_and_remain_for_diagnosis() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    std::fs::write(&executable, b"old").unwrap();
    std::fs::write(&state, vec![b'x'; 64 * 1024 + 1]).unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o600)).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    assert!(matches!(
        updater.recover_on_startup("1").await,
        Err(UpdateError::InvalidMarker { .. })
    ));
    assert_eq!(std::fs::metadata(&state).unwrap().len(), 64 * 1024 + 1);
}

async fn validated(
    updater: &Updater,
    script: &[u8],
    version: &str,
) -> soma_self_update::ValidatedArtifact {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = std::fs::metadata(updater.layout().executable()) {
        let mode = metadata.permissions().mode();
        if mode & 0o111 == 0 {
            std::fs::set_permissions(
                updater.layout().executable(),
                std::fs::Permissions::from_mode(mode | 0o700),
            )
            .unwrap();
        }
    }
    let directive = UpdateDirective::new(version, "/binary", digest(script)).unwrap();
    let staged = updater.stage(script, &directive).await.unwrap();
    updater.validate(staged).await.unwrap()
}

#[tokio::test]
async fn complete_install_and_confirmation_transaction() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let artifact = validated(&updater, new, "2.0.0").await;
    assert!(matches!(
        updater.install(artifact, "1.0.0").await.unwrap(),
        InstallOutcome::RestartRequired { .. }
    ));
    assert_eq!(std::fs::read(&executable).unwrap(), new);
    assert!(state.exists());
    assert_eq!(
        updater.confirm_success("2.0.0").await.unwrap(),
        ConfirmationOutcome::Confirmed {
            version: "2.0.0".into()
        }
    );
    assert!(!state.exists());
    assert_eq!(
        std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("rollback"))
            .count(),
        0
    );
}

#[tokio::test]
async fn confirmation_rejects_same_version_replacement_and_preserves_rollback() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let replacement = temp.path().join("replacement");
    let state = temp.path().join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    let changed = b"#!/bin/sh\necho 'changed example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    updater
        .install(validated(&updater, new, "2.0.0").await, "1.0.0")
        .await
        .unwrap();
    let marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state).unwrap()).unwrap();
    let backup = std::path::PathBuf::from(marker["backup"].as_str().unwrap());
    std::fs::write(&replacement, changed).unwrap();
    std::fs::rename(&replacement, &executable).unwrap();

    assert!(matches!(
        updater.confirm_success("2.0.0").await,
        Err(UpdateError::DigestMismatch { .. })
    ));
    assert_eq!(std::fs::read(&executable).unwrap(), changed);
    assert_eq!(std::fs::read(&backup).unwrap(), old);
    assert!(state.exists());
}

#[tokio::test]
async fn rolls_back_after_unconfirmed_restart_limit() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    let policy = UpdatePolicy::default()
        .with_max_unconfirmed_restarts(2)
        .unwrap();
    let updater = Updater::new(UpdateLayout::new(&executable, &state), policy);
    updater
        .install(validated(&updater, new, "2.0.0").await, "1.0.0")
        .await
        .unwrap();
    assert!(matches!(
        updater.recover_on_startup("2.0.0").await.unwrap(),
        RecoveryAction::PendingUpdate { attempts: 1, .. }
    ));
    assert!(matches!(
        updater.recover_on_startup("2.0.0").await.unwrap(),
        RecoveryAction::PendingUpdate { attempts: 2, .. }
    ));
    assert!(matches!(
        updater.recover_on_startup("2.0.0").await.unwrap(),
        RecoveryAction::RollbackInstalled { .. }
    ));
    assert_eq!(std::fs::read(&executable).unwrap(), old);
    assert!(!state.exists());
}

#[tokio::test]
async fn lock_and_corrupt_recovery_state_fail_closed() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    std::fs::write(&executable, b"old").unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let lock_path = temp.path().join("update.json.lock");
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    lock.try_lock_exclusive().unwrap();
    assert!(matches!(
        updater.recover_on_startup("1").await,
        Err(UpdateError::UpdateInProgress { .. })
    ));
    lock.unlock().unwrap();

    std::fs::write(&state, b"not json").unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(matches!(
        updater.recover_on_startup("1").await,
        Err(UpdateError::InvalidMarker { .. })
    ));
    assert!(state.exists());
}

#[tokio::test]
async fn owned_legacy_lock_permissions_are_repaired_before_use() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let state_lock = temp.path().join("update.json.lock");
    let executable_lock = temp.path().join(".example.update.lock");
    std::fs::write(&executable, b"old").unwrap();
    std::fs::write(&state_lock, b"").unwrap();
    std::fs::write(&executable_lock, b"").unwrap();
    std::fs::set_permissions(&state_lock, std::fs::Permissions::from_mode(0o4600)).unwrap();
    std::fs::set_permissions(&executable_lock, std::fs::Permissions::from_mode(0o2600)).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );

    assert_eq!(
        updater.recover_on_startup("1.0.0").await.unwrap(),
        RecoveryAction::NoPendingUpdate
    );
    assert_eq!(
        std::fs::metadata(&state_lock).unwrap().permissions().mode() & 0o7777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(&executable_lock)
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o600
    );
}

#[tokio::test]
async fn transaction_lock_rejects_symlinks_and_non_regular_files() {
    use nix::sys::stat::Mode;

    for attack in ["symlink", "fifo"] {
        let temp = tempdir().unwrap();
        let executable = temp.path().join("example");
        let state = temp.path().join("update.json");
        let lock = temp.path().join("update.json.lock");
        std::fs::write(&executable, b"old").unwrap();
        if attack == "symlink" {
            let foreign = temp.path().join("foreign");
            std::fs::write(&foreign, b"foreign bytes").unwrap();
            std::os::unix::fs::symlink(&foreign, &lock).unwrap();
        } else {
            nix::unistd::mkfifo(&lock, Mode::S_IRUSR | Mode::S_IWUSR).unwrap();
        }
        let updater = Updater::new(
            UpdateLayout::new(&executable, &state),
            UpdatePolicy::default(),
        );

        assert!(updater.recover_on_startup("1.0.0").await.is_err());
        assert_eq!(std::fs::read(&executable).unwrap(), b"old");
        assert!(!state.exists());
    }
}

#[tokio::test]
async fn symlinked_state_paths_are_rejected_across_process_construction() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let state_alias = temp.path().join("update-alias.json");
    std::fs::write(&executable, b"old").unwrap();
    std::os::unix::fs::symlink(&state, &state_alias).unwrap();
    let aliased = Updater::new(
        UpdateLayout::new(&executable, &state_alias),
        UpdatePolicy::default(),
    );

    match aliased.recover_on_startup("1").await {
        Err(UpdateError::Io { path, source }) => {
            assert_eq!(path, state_alias);
            assert_eq!(source.kind(), std::io::ErrorKind::InvalidInput);
            assert_eq!(
                source.to_string(),
                "state path must not contain symlinked components"
            );
        }
        other => panic!("unexpected construction diagnostic: {other:?}"),
    }
    std::fs::remove_file(&state_alias).unwrap();
    let retarget = temp.path().join("retarget");
    std::fs::create_dir(&retarget).unwrap();
    std::os::unix::fs::symlink(retarget.join("update.json"), &state_alias).unwrap();
    let reconstructed = Updater::new(
        UpdateLayout::new(&executable, &state_alias),
        UpdatePolicy::default(),
    );
    assert!(matches!(
        reconstructed.recover_on_startup("1").await,
        Err(UpdateError::Io { source, .. })
            if source.kind() == std::io::ErrorKind::InvalidInput
    ));
}

#[tokio::test]
async fn state_symlink_introduced_after_construction_is_rejected() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let retarget = temp.path().join("other.json");
    std::fs::write(&executable, b"old").unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    std::os::unix::fs::symlink(&retarget, &state).unwrap();

    assert!(matches!(
        updater.recover_on_startup("1").await,
        Err(UpdateError::Io { source, .. })
            if source.kind() == std::io::ErrorKind::InvalidInput
    ));
    assert!(!temp.path().join(".example.update.lock").exists());
    assert!(!state.with_extension("json.lock").exists());
}

#[tokio::test]
async fn executable_leaf_symlink_is_rejected_before_staging_or_recovery() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let executable_alias = temp.path().join("example-alias");
    let state = temp.path().join("update.json");
    std::fs::write(&executable, b"old").unwrap();
    std::os::unix::fs::symlink(&executable, &executable_alias).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable_alias, &state),
        UpdatePolicy::default(),
    );
    let body = b"new";
    let directive = UpdateDirective::new("2", "/binary", digest(body)).unwrap();

    assert!(matches!(
        updater.stage(&body[..], &directive).await,
        Err(UpdateError::InvalidPolicy(
            "executable path must not be a symlink"
        ))
    ));
    assert!(matches!(
        updater.recover_on_startup("1").await,
        Err(UpdateError::InvalidPolicy(
            "executable path must not be a symlink"
        ))
    ));
    assert!(
        std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().contains(".update-"))
    );
    assert_eq!(std::fs::read(executable).unwrap(), b"old");
}

#[tokio::test]
async fn running_version_mismatch_retains_recovery_state() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    updater
        .install(validated(&updater, new, "2.0.0").await, "1.0.0")
        .await
        .unwrap();
    assert!(matches!(
        updater.confirm_success("1.5.0").await,
        Err(UpdateError::RunningVersionMismatch { .. })
    ));
    assert!(state.exists());
    let marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state).unwrap()).unwrap();
    let backup = std::path::PathBuf::from(marker["backup"].as_str().unwrap());
    assert!(matches!(
        updater.recover_on_startup("1.5.0").await,
        Err(UpdateError::RunningVersionMismatch { .. })
    ));
    assert_eq!(std::fs::read(&executable).unwrap(), new);
    assert!(state.exists());
    assert_eq!(std::fs::read(backup).unwrap(), old);
}

#[tokio::test]
async fn confirmation_clears_authoritative_marker_before_backup_cleanup() {
    let temp = tempdir().unwrap();
    let binary_dir = temp.path().join("bin");
    let state_dir = temp.path().join("state");
    std::fs::create_dir(&binary_dir).unwrap();
    std::fs::create_dir(&state_dir).unwrap();
    let executable = binary_dir.join("example");
    let state = state_dir.join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    updater
        .install(validated(&updater, new, "2.0.0").await, "1.0.0")
        .await
        .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&binary_dir, std::fs::Permissions::from_mode(0o500)).unwrap();
    assert!(updater.confirm_success("2.0.0").await.is_err());
    std::fs::set_permissions(&binary_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(!state.exists(), "confirmation marker must be cleared first");
    assert_eq!(
        updater.recover_on_startup("2.0.0").await.unwrap(),
        RecoveryAction::NoPendingUpdate
    );
}

#[tokio::test]
async fn missing_backup_is_diagnostic_and_preserves_marker() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default()
            .with_max_unconfirmed_restarts(1)
            .unwrap(),
    );
    updater
        .install(validated(&updater, new, "2.0.0").await, "1.0.0")
        .await
        .unwrap();
    let marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state).unwrap()).unwrap();
    std::fs::remove_file(marker["backup"].as_str().unwrap()).unwrap();
    assert!(matches!(
        updater.recover_on_startup("2.0.0").await,
        Err(UpdateError::MissingRollback { .. })
    ));
    assert!(state.exists());
}

#[tokio::test]
async fn failed_install_preserves_original_and_cleans_created_backup() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("state-is-a-directory");
    std::fs::create_dir(&state).unwrap();
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let result = updater
        .install(validated(&updater, new, "2.0.0").await, "1.0.0")
        .await;
    assert!(result.is_err());
    assert_eq!(std::fs::read(&executable).unwrap(), old);
    assert_eq!(
        std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("rollback"))
            .count(),
        0
    );
}

#[tokio::test]
async fn second_install_preserves_last_confirmed_rollback_chain() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let v1 = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let v2 = b"#!/bin/sh\necho 'example 2.0.0'\n";
    let v3 = b"#!/bin/sh\necho 'example 3.0.0'\n";
    std::fs::write(&executable, v1).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default()
            .with_max_unconfirmed_restarts(1)
            .unwrap(),
    );
    updater
        .install(validated(&updater, v2, "2.0.0").await, "1.0.0")
        .await
        .unwrap();
    let original_marker = std::fs::read(&state).unwrap();
    let marker: serde_json::Value = serde_json::from_slice(&original_marker).unwrap();
    let backup = std::path::PathBuf::from(marker["backup"].as_str().unwrap());
    let result = updater
        .install(validated(&updater, v3, "3.0.0").await, "2.0.0")
        .await;
    assert!(matches!(
        result,
        Err(UpdateError::PendingUpdateExists { .. })
    ));
    assert_eq!(std::fs::read(&state).unwrap(), original_marker);
    assert_eq!(std::fs::read(&backup).unwrap(), v1);
    assert!(matches!(
        updater.recover_on_startup("2.0.0").await.unwrap(),
        RecoveryAction::PendingUpdate { .. }
    ));
    assert!(matches!(
        updater.recover_on_startup("2.0.0").await.unwrap(),
        RecoveryAction::RollbackInstalled { .. }
    ));
    assert_eq!(std::fs::read(&executable).unwrap(), v1);
}

#[tokio::test]
async fn layout_collisions_are_rejected_before_filesystem_mutation() {
    for executable_is_state in [true, false] {
        let temp = tempdir().unwrap();
        let state = temp.path().join("update.json");
        let executable = if executable_is_state {
            state.clone()
        } else {
            temp.path().join("update.json.lock")
        };
        let original = b"#!/bin/sh\necho 'example 1.0.0'\n";
        let update = b"#!/bin/sh\necho 'example 2.0.0'\n";
        std::fs::write(&executable, original).unwrap();
        let updater = Updater::new(
            UpdateLayout::new(&executable, &state),
            UpdatePolicy::default(),
        );
        let directive = UpdateDirective::new("2.0.0", "/binary", digest(update)).unwrap();
        assert!(matches!(
            updater.stage(&update[..], &directive).await,
            Err(UpdateError::InvalidLayout { .. })
        ));
        assert_eq!(std::fs::read(&executable).unwrap(), original);
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }
}

#[tokio::test]
async fn casefolded_authority_alias_is_rejected_before_filesystem_mutation() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("agent");
    let state = temp.path().join(".AGENT.UPDATE.AUTHORITY");
    std::fs::write(&executable, b"old").unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );

    assert!(matches!(
        updater.recover_on_startup("1").await,
        Err(UpdateError::InvalidLayout { .. })
    ));
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn idle_state_authority_can_be_migrated_explicitly() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let old_state = temp.path().join("old-update.json");
    let new_state = temp.path().join("new-update.json");
    std::fs::write(&executable, b"old").unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &old_state),
        UpdatePolicy::default(),
    );
    assert_eq!(
        updater.recover_on_startup("1").await.unwrap(),
        RecoveryAction::NoPendingUpdate
    );

    let migrated = updater
        .migrate_state_file(&new_state)
        .await
        .unwrap()
        .into_updater();

    assert_eq!(migrated.layout().state_file(), new_state);
    assert_eq!(
        migrated.recover_on_startup("1").await.unwrap(),
        RecoveryAction::NoPendingUpdate
    );
    assert!(matches!(
        updater.recover_on_startup("1").await,
        Err(UpdateError::InvalidLayout { .. })
    ));
}

#[tokio::test]
async fn state_authority_migration_refuses_a_pending_update() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let destination = temp.path().join("migrated.json");
    let old = b"#!/bin/sh\necho 'example 1.0.0'\n";
    let new = b"#!/bin/sh\necho 'example 2.0.0'\n";
    std::fs::write(&executable, old).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    updater
        .install(validated(&updater, new, "2.0.0").await, "1.0.0")
        .await
        .unwrap();

    assert!(matches!(
        updater.migrate_state_file(&destination).await,
        Err(UpdateError::StateMigrationBlocked { path, .. }) if path == state
    ));
}

#[tokio::test]
async fn state_authority_migration_refuses_indeterminate_marker_temporary_state() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let state_temp = state.with_file_name("update.json.tmp");
    let destination = temp.path().join("migrated.json");
    std::fs::write(&executable, b"old").unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    updater.recover_on_startup("1").await.unwrap();
    std::fs::write(&state_temp, b"partial marker").unwrap();

    assert!(matches!(
        updater.migrate_state_file(&destination).await,
        Err(UpdateError::StateMigrationBlocked { path, .. }) if path == state_temp
    ));
    assert_eq!(
        updater.recover_on_startup("1").await.unwrap(),
        RecoveryAction::NoPendingUpdate
    );
}

#[tokio::test]
async fn state_authority_migration_refuses_recovery_artifacts() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("example");
    let state = temp.path().join("update.json");
    let destination = temp.path().join("migrated.json");
    let staged = temp
        .path()
        .join(format!(".example.update-{}-1.part", std::process::id()));
    std::fs::write(&executable, b"old").unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    updater.recover_on_startup("1").await.unwrap();
    std::fs::write(&staged, b"in-flight").unwrap();

    assert!(matches!(
        updater.migrate_state_file(&destination).await,
        Err(UpdateError::StateMigrationBlocked { path, .. }) if path == staged
    ));
}
