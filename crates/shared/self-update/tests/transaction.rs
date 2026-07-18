#![cfg(unix)]

use std::fs::OpenOptions;

use fs2::FileExt;
use sha2::{Digest, Sha256};
use soma_self_update::{
    ConfirmationOutcome, InstallOutcome, RecoveryAction, UpdateDirective, UpdateError,
    UpdateLayout, UpdatePolicy, Updater,
};
use tempfile::tempdir;

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn validated(
    updater: &Updater,
    script: &[u8],
    version: &str,
) -> soma_self_update::ValidatedArtifact {
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
    assert!(matches!(
        updater.recover_on_startup("1").await,
        Err(UpdateError::InvalidMarker { .. })
    ));
    assert!(state.exists());
}

#[tokio::test]
async fn stale_and_mismatched_versions_do_not_replace_bytes() {
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
    assert!(matches!(
        updater.recover_on_startup("1.5.0").await.unwrap(),
        RecoveryAction::StaleMarkerRemoved { .. }
    ));
    assert_eq!(std::fs::read(&executable).unwrap(), new);
    assert!(!state.exists());
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
        updater.recover_on_startup("2.0.0").await.unwrap(),
        RecoveryAction::PendingUpdate { .. }
    ));
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
