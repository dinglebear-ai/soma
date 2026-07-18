use std::path::Path;

use crate::{Result, UpdateError, Updater, ValidatedArtifact};

use super::TestFailpoint;
use super::transaction_io::{
    ensure_validated_artifact_mode, hash_stable_validated_artifact, remove_and_sync,
    remove_if_present_and_sync, restore_validated_artifact_mode,
};

pub(super) fn cleanup_prepared_marker_failure(
    updater: &Updater,
    state: &Path,
    backup: &Path,
    operation: UpdateError,
) -> UpdateError {
    if updater.failpoint_active(TestFailpoint::AfterPreparedMarkerRenameWithStateCleanupFailure) {
        return combined_error(
            operation,
            UpdateError::io(
                state,
                std::io::Error::other("injected prepared-marker cleanup failure"),
            ),
        );
    }
    if let Err(cleanup) = remove_if_present_and_sync(state) {
        return combined_error(operation, cleanup);
    }
    if let Err(cleanup) = remove_and_sync(backup) {
        return combined_error(operation, cleanup);
    }
    operation
}

pub(super) fn validate_or_cleanup(
    updater: &Updater,
    validated: &ValidatedArtifact,
    validated_path: &Path,
    state: &Path,
    backup: &Path,
) -> Result<()> {
    let validation = (|| {
        if updater.failpoint_active(TestFailpoint::PostMarkerModeFailure)
            || updater.failpoint_active(TestFailpoint::PostMarkerModeFailureWithStateCleanupFailure)
        {
            return Err(UpdateError::ArtifactIdentityChanged {
                path: validated_path.to_owned(),
            });
        }
        restore_validated_artifact_mode(validated, validated_path)?;
        let final_digest = hash_stable_validated_artifact(validated, validated_path)?;
        let forced_digest_failure = updater
            .failpoint_active(TestFailpoint::PostMarkerDigestFailure)
            || updater
                .failpoint_active(TestFailpoint::PostMarkerDigestFailureWithBackupCleanupFailure);
        if final_digest != validated.sha256() || forced_digest_failure {
            return Err(UpdateError::DigestMismatch {
                expected: validated.sha256().to_owned(),
                actual: if forced_digest_failure {
                    "injected post-marker digest mismatch".into()
                } else {
                    final_digest
                },
            });
        }
        ensure_validated_artifact_mode(validated, validated_path)
    })();
    match validation {
        Ok(()) => Ok(()),
        Err(operation) => Err(cleanup_failure(updater, state, backup, operation)),
    }
}

fn cleanup_failure(
    updater: &Updater,
    state: &Path,
    backup: &Path,
    operation: UpdateError,
) -> UpdateError {
    if updater.failpoint_active(TestFailpoint::PostMarkerModeFailureWithStateCleanupFailure) {
        return combined_error(
            operation,
            UpdateError::io(
                state,
                std::io::Error::other("injected state cleanup failure"),
            ),
        );
    }
    if let Err(cleanup) = remove_and_sync(state) {
        return combined_error(operation, cleanup);
    }
    if updater.failpoint_active(TestFailpoint::PostMarkerDigestFailureWithBackupCleanupFailure) {
        return combined_error(
            operation,
            UpdateError::io(
                backup,
                std::io::Error::other("injected rollback cleanup failure"),
            ),
        );
    }
    if let Err(cleanup) = remove_and_sync(backup) {
        return combined_error(operation, cleanup);
    }
    operation
}

fn combined_error(operation: UpdateError, cleanup: UpdateError) -> UpdateError {
    UpdateError::TransactionCleanupFailed {
        operation: Box::new(operation),
        cleanup: Box::new(cleanup),
    }
}
