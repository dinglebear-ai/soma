use std::path::PathBuf;

use crate::{RecoveryAction, Result, UpdateError, Updater, ValidatedArtifact};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallOutcome {
    /// The swap and durable installed marker completed; restart into the new executable.
    RestartRequired {
        executable: PathBuf,
        from: String,
        to: String,
    },
    /// The executable was swapped, but a subsequent durability or marker step failed.
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

impl Updater {
    pub async fn migrate_state_file(&self, _new_state_file: impl Into<PathBuf>) -> Result<Self> {
        Err(UpdateError::UnsupportedPlatform)
    }

    pub async fn install(
        &self,
        _validated: ValidatedArtifact,
        _previous_version: impl Into<String>,
    ) -> Result<InstallOutcome> {
        Err(UpdateError::UnsupportedPlatform)
    }

    pub async fn recover_on_startup(&self, _running_version: &str) -> Result<RecoveryAction> {
        Err(UpdateError::UnsupportedPlatform)
    }

    pub async fn confirm_success(&self, _running_version: &str) -> Result<ConfirmationOutcome> {
        Err(UpdateError::UnsupportedPlatform)
    }
}
