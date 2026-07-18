use std::path::PathBuf;

use super::{ConfirmationOutcome, InstallOutcome};
use crate::{RecoveryAction, Result, UpdateError, Updater, ValidatedArtifact};

impl Updater {
    pub async fn install(
        &self,
        validated: ValidatedArtifact,
        previous_version: impl Into<String>,
    ) -> Result<InstallOutcome> {
        let updater = self.clone();
        let error_path = self.layout().state_file().to_path_buf();
        let previous = previous_version.into();
        blocking_transaction(error_path, move || {
            updater.install_sync(validated, previous)
        })
        .await
    }

    pub async fn recover_on_startup(&self, running_version: &str) -> Result<RecoveryAction> {
        let updater = self.clone();
        let error_path = self.layout().state_file().to_path_buf();
        let running_version = running_version.to_owned();
        blocking_transaction(error_path, move || {
            updater.recover_on_startup_sync(&running_version)
        })
        .await
    }

    pub async fn confirm_success(&self, running_version: &str) -> Result<ConfirmationOutcome> {
        let updater = self.clone();
        let error_path = self.layout().state_file().to_path_buf();
        let running_version = running_version.to_owned();
        blocking_transaction(error_path, move || {
            updater.confirm_success_sync(&running_version)
        })
        .await
    }
}

async fn blocking_transaction<T, F>(error_path: PathBuf, operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| {
            UpdateError::io(
                error_path,
                std::io::Error::other(format!("blocking update transaction failed: {error}")),
            )
        })?
}
