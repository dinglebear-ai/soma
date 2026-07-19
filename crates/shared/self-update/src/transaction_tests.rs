use std::path::PathBuf;

use super::*;
use crate::{UpdateDirective, UpdateLayout, UpdatePolicy};
use tempfile::tempdir;

#[path = "transaction_lock_tests.rs"]
mod lock_tests;

async fn updater_and_artifact(
    max_restarts: u32,
) -> (
    tempfile::TempDir,
    Updater,
    ValidatedArtifact,
    Vec<u8>,
    Vec<u8>,
) {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let executable = temp.path().join("agent");
    let state = temp.path().join("update.json");
    let old = b"#!/bin/sh\necho 'agent 1.0.0'\n".to_vec();
    let new = b"#!/bin/sh\necho 'agent 2.0.0'\n".to_vec();
    std::fs::write(&executable, &old).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default()
            .with_max_unconfirmed_restarts(max_restarts)
            .unwrap(),
    );
    let directive = UpdateDirective::new("2.0.0", "/agent", hash_bytes(&new)).unwrap();
    let staged = updater.stage(&new[..], &directive).await.unwrap();
    let validated = updater.validate(staged).await.unwrap();
    (temp, updater, validated, old, new)
}

fn hash_bytes(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn rollback_artifacts(updater: &Updater) -> Vec<PathBuf> {
    std::fs::read_dir(updater.layout().executable().parent().unwrap())
        .unwrap()
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().contains(".rollback-"))
        })
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn prepared_marker_parent_sync_failure_cleans_state_before_backup() {
    let (_temp, updater, artifact, old, _new) = updater_and_artifact(1).await;
    let staged = artifact.path().to_path_buf();
    updater.set_test_failpoint(TestFailpoint::AfterPreparedMarkerRename);

    let error = updater.install(artifact, "1.0.0").await.unwrap_err();

    assert!(matches!(error, UpdateError::Io { .. }));
    assert_eq!(std::fs::read(updater.layout().executable()).unwrap(), old);
    assert!(!updater.layout().state_file().exists());
    assert!(rollback_artifacts(&updater).is_empty());
    assert!(!staged.exists());
}

#[tokio::test(flavor = "current_thread")]
async fn prepared_marker_cleanup_failure_retains_authoritative_backup() {
    let (_temp, updater, artifact, old, _new) = updater_and_artifact(1).await;
    let staged = artifact.path().to_path_buf();
    updater.set_test_failpoint(TestFailpoint::AfterPreparedMarkerRenameWithStateCleanupFailure);

    let error = updater.install(artifact, "1.0.0").await.unwrap_err();

    let UpdateError::TransactionCleanupFailed { operation, cleanup } = error else {
        panic!("expected combined marker-write and cleanup error");
    };
    assert!(matches!(*operation, UpdateError::Io { .. }));
    assert!(matches!(*cleanup, UpdateError::Io { .. }));
    assert_eq!(std::fs::read(updater.layout().executable()).unwrap(), old);
    assert!(updater.layout().state_file().exists());
    assert_eq!(rollback_artifacts(&updater).len(), 1);
    assert!(!staged.exists());

    updater.set_test_failpoint(TestFailpoint::None);
    assert_eq!(
        updater.recover_on_startup("1.0.0").await.unwrap(),
        RecoveryAction::NoPendingUpdate
    );
    assert!(!updater.layout().state_file().exists());
    assert!(rollback_artifacts(&updater).is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn post_marker_mode_and_digest_failures_remove_state_then_backup() {
    for failpoint in [
        TestFailpoint::PostMarkerModeFailure,
        TestFailpoint::PostMarkerDigestFailure,
    ] {
        let (_temp, updater, artifact, old, _new) = updater_and_artifact(1).await;
        let staged = artifact.path().to_path_buf();
        updater.set_test_failpoint(failpoint);

        let error = updater.install(artifact, "1.0.0").await.unwrap_err();
        match failpoint {
            TestFailpoint::PostMarkerModeFailure => {
                assert!(matches!(error, UpdateError::ArtifactIdentityChanged { .. }));
            }
            TestFailpoint::PostMarkerDigestFailure => {
                assert!(matches!(error, UpdateError::DigestMismatch { .. }));
            }
            _ => unreachable!(),
        }
        assert_eq!(std::fs::read(updater.layout().executable()).unwrap(), old);
        assert!(!updater.layout().state_file().exists());
        assert!(rollback_artifacts(&updater).is_empty());
        assert!(!staged.exists());
    }
}

#[tokio::test(flavor = "current_thread")]
async fn post_marker_cleanup_failures_preserve_primary_and_authoritative_ordering() {
    for (failpoint, state_remains, backup_remains) in [
        (
            TestFailpoint::PostMarkerModeFailureWithStateCleanupFailure,
            true,
            true,
        ),
        (
            TestFailpoint::PostMarkerDigestFailureWithBackupCleanupFailure,
            false,
            true,
        ),
    ] {
        let (_temp, updater, artifact, old, _new) = updater_and_artifact(1).await;
        let staged = artifact.path().to_path_buf();
        updater.set_test_failpoint(failpoint);

        let error = updater.install(artifact, "1.0.0").await.unwrap_err();
        let UpdateError::TransactionCleanupFailed { operation, cleanup } = error else {
            panic!("expected combined operation and cleanup error");
        };
        match failpoint {
            TestFailpoint::PostMarkerModeFailureWithStateCleanupFailure => {
                assert!(matches!(
                    *operation,
                    UpdateError::ArtifactIdentityChanged { .. }
                ));
            }
            TestFailpoint::PostMarkerDigestFailureWithBackupCleanupFailure => {
                assert!(matches!(*operation, UpdateError::DigestMismatch { .. }));
            }
            _ => unreachable!(),
        }
        assert!(matches!(*cleanup, UpdateError::Io { .. }));
        assert_eq!(std::fs::read(updater.layout().executable()).unwrap(), old);
        assert_eq!(updater.layout().state_file().exists(), state_remains);
        assert_eq!(!rollback_artifacts(&updater).is_empty(), backup_remains);
        assert!(!staged.exists());

        updater.set_test_failpoint(TestFailpoint::None);
        assert_eq!(
            updater.recover_on_startup("1.0.0").await.unwrap(),
            RecoveryAction::NoPendingUpdate
        );
        assert!(!updater.layout().state_file().exists());
        if state_remains {
            assert!(rollback_artifacts(&updater).is_empty());
        } else {
            assert_eq!(
                rollback_artifacts(&updater).len(),
                1,
                "the current process still owns this diagnostic orphan"
            );
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn failpoints_after_marker_and_swap_recover_idempotently() {
    let (_temp, updater, artifact, _old, _new) = updater_and_artifact(1).await;
    updater.set_test_failpoint(TestFailpoint::AfterMarkerSync);
    assert!(updater.install(artifact, "1.0.0").await.is_err());
    updater.set_test_failpoint(TestFailpoint::None);
    assert_eq!(
        updater.recover_on_startup("1.0.0").await.unwrap(),
        RecoveryAction::NoPendingUpdate
    );

    let (_temp, updater, artifact, _old, _new) = updater_and_artifact(1).await;
    updater.set_test_failpoint(TestFailpoint::AfterSwap);
    assert!(matches!(
        updater.install(artifact, "1.0.0").await.unwrap(),
        InstallOutcome::RestartRequiredIndeterminate {
            ref from,
            ref to,
            ref error,
            ..
        } if from == "1.0.0" && to == "2.0.0" && !error.is_empty()
    ));
    updater.set_test_failpoint(TestFailpoint::None);
    assert_eq!(
        updater.recover_on_startup("2.0.0").await.unwrap(),
        RecoveryAction::PendingUpdate {
            target: "2.0.0".into(),
            attempts: 1,
            max_attempts: 1,
        }
    );
}

#[tokio::test(flavor = "current_thread")]
async fn failpoint_after_rollback_rename_recovers_idempotently() {
    let (_temp, updater, artifact, old, _new) = updater_and_artifact(1).await;
    updater.install(artifact, "1.0.0").await.unwrap();
    updater.recover_on_startup("2.0.0").await.unwrap();
    updater.set_test_failpoint(TestFailpoint::AfterRollbackRename);
    assert!(updater.recover_on_startup("2.0.0").await.is_err());
    updater.set_test_failpoint(TestFailpoint::None);
    assert!(matches!(
        updater.recover_on_startup("1.0.0").await.unwrap(),
        RecoveryAction::RollbackInstalled { .. }
    ));
    assert_eq!(std::fs::read(updater.layout().executable()).unwrap(), old);
}

#[tokio::test(flavor = "current_thread")]
async fn failed_rename_cleanup_is_authoritative_state_first() {
    for (failpoint, expected_backups) in [
        (TestFailpoint::FailedRenameAfterMarkerCleanup, 1),
        (TestFailpoint::FailedRenameAfterBackupCleanup, 0),
    ] {
        let (_temp, updater, artifact, old, _new) = updater_and_artifact(1).await;
        updater.set_test_failpoint(failpoint);
        assert!(updater.install(artifact, "1.0.0").await.is_err());
        updater.set_test_failpoint(TestFailpoint::None);
        assert!(!updater.layout().state_file().exists());
        assert_eq!(std::fs::read(updater.layout().executable()).unwrap(), old);
        let backup_count = std::fs::read_dir(updater.layout().executable().parent().unwrap())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".rollback-"))
            .count();
        assert_eq!(backup_count, expected_backups);
        assert_eq!(
            updater.recover_on_startup("1.0.0").await.unwrap(),
            RecoveryAction::NoPendingUpdate
        );
    }
}

#[tokio::test]
async fn failpoint_on_one_updater_does_not_affect_another_updater() {
    let (_first_temp, first, first_artifact, _old, _new) = updater_and_artifact(1).await;
    let (_second_temp, second, second_artifact, _old, _new) = updater_and_artifact(1).await;
    first.set_test_failpoint(TestFailpoint::AfterMarkerSync);

    assert!(second.install(second_artifact, "1.0.0").await.is_ok());
    assert!(first.install(first_artifact, "1.0.0").await.is_err());
}

#[tokio::test]
async fn marker_temp_crash_is_reclaimed_under_the_state_lock() {
    let (_temp, updater, artifact, old, _new) = updater_and_artifact(1).await;
    let marker_temp = suffix_path(updater.layout().state_file(), ".tmp");
    updater.set_test_failpoint(TestFailpoint::AfterMarkerTempSync);

    assert!(updater.install(artifact, "1.0.0").await.is_err());
    assert!(marker_temp.is_file());
    assert_eq!(std::fs::read(updater.layout().executable()).unwrap(), old);

    updater.set_test_failpoint(TestFailpoint::None);
    assert_eq!(
        updater.recover_on_startup("1.0.0").await.unwrap(),
        RecoveryAction::NoPendingUpdate
    );
    assert!(!marker_temp.exists());
}

#[test]
fn marker_temp_owner_matches_service_even_when_directory_owner_differs() {
    let root_owned_directory = 0;
    let service_effective_uid = 1000;
    let service_created_temp = service_effective_uid;

    assert_ne!(root_owned_directory, service_effective_uid);
    assert!(marker_temp_owner_is_valid(
        service_created_temp,
        service_effective_uid
    ));
    assert!(!marker_temp_owner_is_valid(
        root_owned_directory,
        service_effective_uid
    ));
}

#[test]
fn generated_backup_must_not_collide_with_transaction_paths() {
    let root = std::path::Path::new("/trusted/bin");
    let executable = root.join("agent");
    let state = root.join("state.json");
    let locks = vec![
        root.join("state.json.lock"),
        root.join(".agent.update.lock"),
    ];
    let marker_temp = root.join("state.json.tmp");
    let staged = root.join(".agent.update-1-1.part");

    for collision in [&state, &locks[0], &locks[1], &marker_temp, &staged] {
        assert!(matches!(
            validate_backup_candidate(
                &executable,
                &state,
                &locks,
                &marker_temp,
                &staged,
                collision
            ),
            Err(UpdateError::InvalidLayout { .. })
        ));
    }
}

#[test]
fn created_marker_and_lock_ignore_permissive_umask() {
    use std::os::unix::fs::PermissionsExt;

    const CHILD_ENV: &str = "SOMA_SELF_UPDATE_UMASK_CHILD";
    if std::env::var_os(CHILD_ENV).is_some() {
        nix::sys::stat::umask(nix::sys::stat::Mode::empty());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let (_temp, updater, artifact, _old, _new) = runtime.block_on(updater_and_artifact(1));
        runtime
            .block_on(updater.install(artifact, "1.0.0"))
            .unwrap();
        let state = updater.layout().state_file();
        let lock = transaction_layout::executable_lock_path(updater.layout().executable()).unwrap();
        assert_eq!(
            std::fs::metadata(state).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(lock).unwrap().permissions().mode() & 0o777,
            0o600
        );
        return;
    }

    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "transaction::tests::created_marker_and_lock_ignore_permissive_umask",
            "--nocapture",
        ])
        .env(CHILD_ENV, "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "umask child failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
