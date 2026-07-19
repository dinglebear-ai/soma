use std::path::PathBuf;

use crate::UpdateError;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy)]
#[repr(u8)]
pub(crate) enum TestFailpoint {
    None,
    AfterMarkerTempSync,
    AfterMarkerSync,
    AfterSwap,
    AfterRollbackRename,
    FailedRenameAfterMarkerCleanup,
    FailedRenameAfterBackupCleanup,
    AfterPreparedMarkerRename,
    AfterPreparedMarkerRenameWithStateCleanupFailure,
    PostMarkerModeFailure,
    PostMarkerDigestFailure,
    PostMarkerModeFailureWithStateCleanupFailure,
    PostMarkerDigestFailureWithBackupCleanupFailure,
    AuthorityAfterPartialWrite,
    AuthorityBeforeFileSync,
    AuthorityBeforeDirectorySync,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallOutcome {
    /// The swap and durable installed marker completed; restart into the new executable.
    RestartRequired {
        executable: PathBuf,
        from: String,
        to: String,
    },
    /// The executable was swapped, but a subsequent durability or marker step failed.
    ///
    /// The caller must restart into `executable` and let startup recovery inspect the
    /// prepared marker. Treating this as an ordinary pre-swap failure can leave the old
    /// process running after its on-disk executable has changed.
    RestartRequiredIndeterminate {
        executable: PathBuf,
        from: String,
        to: String,
        error: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfirmationOutcome {
    NoPendingUpdate,
    Confirmed { version: String },
}

pub(super) fn indeterminate_restart(
    executable: PathBuf,
    previous: String,
    target: String,
    error: UpdateError,
) -> InstallOutcome {
    InstallOutcome::RestartRequiredIndeterminate {
        executable,
        from: previous,
        to: target,
        error: error.to_string(),
    }
}
