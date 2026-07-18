use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::Ordering;

#[cfg(test)]
use sha2::{Digest, Sha256};

use crate::{RecoveryAction, Result, UpdateError, Updater, ValidatedArtifact};

#[path = "transaction_artifacts.rs"]
mod artifacts;
#[path = "transaction_async.rs"]
mod asynchronous;
#[path = "transaction_marker.rs"]
mod marker;
#[path = "transaction_io.rs"]
mod transaction_io;
#[path = "transaction_layout.rs"]
mod transaction_layout;
use artifacts::{cleanup_owned_artifacts, validate_backup_candidate, validate_rollback_backup};
#[cfg(test)]
use marker::marker_temp_owner_is_valid;
use marker::{
    Marker, MarkerPhase, cleanup_marker_temp, preflight_marker_lifecycle, read_marker, write_marker,
};
use transaction_io::{
    absolute, create_backup, hash_file, hash_stable_validated_artifact, remove_and_sync,
    remove_file, remove_if_present_and_sync, suffix_path, sync_parent, unique_backup,
};

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy)]
#[repr(u8)]
pub(super) enum TestFailpoint {
    None,
    AfterMarkerTempSync,
    AfterMarkerSync,
    AfterSwap,
    AfterRollbackRename,
    FailedRenameAfterMarkerCleanup,
    FailedRenameAfterBackupCleanup,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallOutcome {
    RestartRequired {
        executable: PathBuf,
        from: String,
        to: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfirmationOutcome {
    NoPendingUpdate,
    Confirmed { version: String },
}

impl Updater {
    #[cfg(test)]
    fn set_test_failpoint(&self, failpoint: TestFailpoint) {
        self.test_failpoint.store(failpoint as u8, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(super) fn failpoint_active(&self, failpoint: TestFailpoint) -> bool {
        self.test_failpoint.load(Ordering::SeqCst) == failpoint as u8
    }

    #[cfg(not(test))]
    pub(super) fn failpoint_active(&self, _failpoint: TestFailpoint) -> bool {
        false
    }

    pub(super) fn maybe_fail(&self, failpoint: TestFailpoint, path: &Path) -> Result<()> {
        if self.failpoint_active(failpoint) {
            return Err(UpdateError::io(
                path,
                std::io::Error::other("injected transaction crash boundary"),
            ));
        }
        Ok(())
    }

    pub(super) fn install_sync(
        &self,
        validated: ValidatedArtifact,
        previous: String,
    ) -> Result<InstallOutcome> {
        let paths = self.validated_layout()?;
        let _lock = self.transaction_lock(&paths.lock)?;
        let executable = paths.executable;
        let state = paths.state;
        let validated_path = absolute(validated.path())?;
        let target = validated.target_version().to_owned();
        let backup = unique_backup(&executable);
        let marker_temp = suffix_path(&state, ".tmp");
        validate_backup_candidate(
            &executable,
            &state,
            &paths.lock,
            &marker_temp,
            &validated_path,
            &backup,
        )?;
        let mut marker = Marker {
            schema_version: 2,
            phase: MarkerPhase::Prepared,
            target: target.clone(),
            previous: previous.clone(),
            executable: executable.clone(),
            backup: backup.clone(),
            staged: validated_path.clone(),
            attempts: 0,
            sha256: validated.sha256().to_owned(),
            previous_sha256: hash_file(&executable)?,
        };
        preflight_marker_lifecycle(&state, &marker)?;

        cleanup_marker_temp(&state)?;
        let staged_metadata = std::fs::symlink_metadata(&validated_path)
            .map_err(|error| UpdateError::io(&validated_path, error))?;
        if !staged_metadata.file_type().is_file() {
            return Err(UpdateError::InvalidStagedArtifact {
                path: validated_path,
            });
        }
        if let Some(marker) = read_marker(&state, &executable)? {
            return Err(UpdateError::PendingUpdateExists {
                path: state,
                target: marker.target,
            });
        }
        cleanup_owned_artifacts(&executable, None, Some(&validated_path))?;
        let actual_digest = hash_stable_validated_artifact(&validated, &validated_path)?;
        if actual_digest != validated.sha256() {
            return Err(UpdateError::DigestMismatch {
                expected: validated.sha256().to_owned(),
                actual: actual_digest,
            });
        }
        create_backup(&executable, &backup, self.policy().backup_strategy())?;
        let backup_digest = hash_file(&backup)?;
        if backup_digest != marker.previous_sha256 {
            remove_file(&backup)?;
            return Err(UpdateError::DigestMismatch {
                expected: marker.previous_sha256,
                actual: backup_digest,
            });
        }
        if let Err(error) = write_marker(self, &state, &marker) {
            remove_file(&backup)?;
            return Err(error);
        }
        self.maybe_fail(TestFailpoint::AfterMarkerSync, &state)?;
        let final_digest = hash_stable_validated_artifact(&validated, &validated_path)?;
        if final_digest != validated.sha256() {
            return Err(UpdateError::DigestMismatch {
                expected: validated.sha256().to_owned(),
                actual: final_digest,
            });
        }
        let forced_rename_failure = self
            .failpoint_active(TestFailpoint::FailedRenameAfterMarkerCleanup)
            || self.failpoint_active(TestFailpoint::FailedRenameAfterBackupCleanup);
        let rename_result = if forced_rename_failure {
            Err(std::io::Error::other("injected final rename failure"))
        } else {
            std::fs::rename(&validated_path, &executable)
        };
        if let Err(source) = rename_result {
            remove_and_sync(&state)?;
            self.maybe_fail(TestFailpoint::FailedRenameAfterMarkerCleanup, &state)?;
            remove_and_sync(&backup)?;
            self.maybe_fail(TestFailpoint::FailedRenameAfterBackupCleanup, &backup)?;
            return Err(UpdateError::io(&executable, source));
        }
        sync_parent(&executable)?;
        self.maybe_fail(TestFailpoint::AfterSwap, &executable)?;
        marker.phase = MarkerPhase::Installed;
        write_marker(self, &state, &marker)?;
        Ok(InstallOutcome::RestartRequired {
            executable,
            from: previous,
            to: target,
        })
    }

    pub(super) fn recover_on_startup_sync(&self, running_version: &str) -> Result<RecoveryAction> {
        let paths = self.validated_layout()?;
        let _lock = self.transaction_lock(&paths.lock)?;
        let state = paths.state;
        cleanup_marker_temp(&state)?;
        let marker = read_marker(&state, &paths.executable)?;
        cleanup_owned_artifacts(
            &paths.executable,
            marker.as_ref().map(|marker| marker.backup.as_path()),
            None,
        )?;
        let Some(mut marker) = marker else {
            return Ok(RecoveryAction::NoPendingUpdate);
        };
        match marker.phase {
            MarkerPhase::Prepared => {
                let executable_digest = hash_file(&marker.executable)?;
                if running_version == marker.previous && executable_digest == marker.previous_sha256
                {
                    abort_prepared(&state, &marker)?;
                    return Ok(RecoveryAction::NoPendingUpdate);
                }
                if running_version == marker.target && executable_digest == marker.sha256 {
                    marker.phase = MarkerPhase::Installed;
                    write_marker(self, &state, &marker)?;
                } else {
                    return Err(version_mismatch(running_version, &marker));
                }
            }
            MarkerPhase::Installed => {
                if marker.target != running_version {
                    return Err(version_mismatch(running_version, &marker));
                }
                let installed_digest = hash_file(&marker.executable)?;
                if installed_digest != marker.sha256 {
                    return Err(UpdateError::DigestMismatch {
                        expected: marker.sha256,
                        actual: installed_digest,
                    });
                }
            }
            MarkerPhase::RollingBack => {
                return resume_rollback(self, &state, marker, running_version);
            }
            MarkerPhase::RolledBack => {
                return finish_rollback(&state, marker, running_version);
            }
        }
        marker.attempts = marker.attempts.saturating_add(1);
        if marker.attempts <= self.policy().max_unconfirmed_restarts() {
            write_marker(self, &state, &marker)?;
            return Ok(RecoveryAction::PendingUpdate {
                target: marker.target,
                attempts: marker.attempts,
                max_attempts: self.policy().max_unconfirmed_restarts(),
            });
        }
        validate_rollback_backup(&state, &marker)?;
        marker.phase = MarkerPhase::RollingBack;
        write_marker(self, &state, &marker)?;
        std::fs::rename(&marker.backup, &marker.executable)
            .map_err(|error| UpdateError::io(&marker.executable, error))?;
        sync_parent(&marker.executable)?;
        self.maybe_fail(TestFailpoint::AfterRollbackRename, &marker.executable)?;
        marker.phase = MarkerPhase::RolledBack;
        write_marker(self, &state, &marker)?;
        finalize_rollback(&state, marker)
    }

    pub(super) fn confirm_success_sync(
        &self,
        running_version: &str,
    ) -> Result<ConfirmationOutcome> {
        let paths = self.validated_layout()?;
        let _lock = self.transaction_lock(&paths.lock)?;
        let state = paths.state;
        cleanup_marker_temp(&state)?;
        let marker = read_marker(&state, &paths.executable)?;
        cleanup_owned_artifacts(
            &paths.executable,
            marker.as_ref().map(|marker| marker.backup.as_path()),
            None,
        )?;
        let Some(mut marker) = marker else {
            return Ok(ConfirmationOutcome::NoPendingUpdate);
        };
        if marker.phase == MarkerPhase::Prepared
            && marker.target == running_version
            && hash_file(&marker.executable)? == marker.sha256
        {
            marker.phase = MarkerPhase::Installed;
            write_marker(self, &state, &marker)?;
        }
        if marker.phase != MarkerPhase::Installed {
            return Err(version_mismatch(running_version, &marker));
        }
        if marker.target != running_version {
            return Err(UpdateError::RunningVersionMismatch {
                running: running_version.to_owned(),
                target: marker.target,
            });
        }
        let installed_digest = hash_file(&marker.executable)?;
        if installed_digest != marker.sha256 {
            return Err(UpdateError::DigestMismatch {
                expected: marker.sha256,
                actual: installed_digest,
            });
        }
        validate_rollback_backup(&state, &marker)?;
        remove_file(&state)?;
        sync_parent(&state)?;
        remove_file(&marker.backup)?;
        sync_parent(&marker.backup)?;
        Ok(ConfirmationOutcome::Confirmed {
            version: running_version.to_owned(),
        })
    }
}

fn version_mismatch(running_version: &str, marker: &Marker) -> UpdateError {
    UpdateError::RunningVersionMismatch {
        running: running_version.to_owned(),
        target: marker.target.clone(),
    }
}

fn abort_prepared(state: &Path, marker: &Marker) -> Result<()> {
    remove_and_sync(state)?;
    remove_if_present_and_sync(&marker.backup)?;
    remove_if_present_and_sync(&marker.staged)
}

fn resume_rollback(
    updater: &Updater,
    state: &Path,
    mut marker: Marker,
    running_version: &str,
) -> Result<RecoveryAction> {
    let executable_digest = hash_file(&marker.executable)?;
    if running_version == marker.previous && executable_digest == marker.previous_sha256 {
        marker.phase = MarkerPhase::RolledBack;
        write_marker(updater, state, &marker)?;
        return finalize_rollback(state, marker);
    }
    if running_version != marker.target || executable_digest != marker.sha256 {
        return Err(version_mismatch(running_version, &marker));
    }
    validate_rollback_backup(state, &marker)?;
    std::fs::rename(&marker.backup, &marker.executable)
        .map_err(|error| UpdateError::io(&marker.executable, error))?;
    sync_parent(&marker.executable)?;
    updater.maybe_fail(TestFailpoint::AfterRollbackRename, &marker.executable)?;
    marker.phase = MarkerPhase::RolledBack;
    write_marker(updater, state, &marker)?;
    finalize_rollback(state, marker)
}

fn finish_rollback(state: &Path, marker: Marker, running_version: &str) -> Result<RecoveryAction> {
    if running_version != marker.previous
        || hash_file(&marker.executable)? != marker.previous_sha256
    {
        return Err(version_mismatch(running_version, &marker));
    }
    finalize_rollback(state, marker)
}

fn finalize_rollback(state: &Path, marker: Marker) -> Result<RecoveryAction> {
    remove_and_sync(state)?;
    Ok(RecoveryAction::RollbackInstalled {
        executable: marker.executable,
        restored_version: marker.previous,
    })
}

#[cfg(test)]
#[path = "transaction_tests.rs"]
mod tests;
