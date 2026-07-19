use std::path::{Path, PathBuf};

use super::artifacts::ensure_no_recovery_artifacts;
use super::authority::{read_state_authority, rewrite_state_authority};
use super::transaction_io::suffix_path;
use crate::{Result, UpdateError, UpdateLayout, Updater, bind_state_identity};

impl Updater {
    pub(super) fn migrate_state_file_sync(&self, new_state_file: PathBuf) -> Result<Self> {
        let old = self.validated_layout()?;
        let new_state = bind_state_identity(&new_state_file)
            .map_err(|error| UpdateError::io(&new_state_file, error))?;
        let migrated = Updater::new(
            UpdateLayout::new(&old.executable, &new_state),
            self.policy().clone(),
        );
        migrated.ensure_layout_bound()?;
        let new = migrated.validated_layout()?;
        if old.executable != new.executable
            || old.authority != new.authority
            || old.authority_temp != new.authority_temp
        {
            return Err(UpdateError::InvalidPolicy(
                "state migration must retain the executable identity",
            ));
        }

        let mut lock_paths = old.locks.clone();
        lock_paths.extend(new.locks.iter().cloned());
        lock_paths.sort();
        lock_paths.dedup();
        let _locks = self.acquire_transaction_locks(&lock_paths)?;
        let authority = read_state_authority(&old.authority, &old.authority_temp)?;
        let authority_state = match authority {
            Some(bound) if bound == old.state => AuthorityState::Current,
            Some(bound) if bound == new.state => AuthorityState::Migrated,
            Some(bound) => {
                return Err(UpdateError::InvalidLayout {
                    first: bound,
                    second: old.state,
                });
            }
            None => AuthorityState::Absent,
        };
        ensure_absent(&old.state, "the current transaction marker exists")?;
        ensure_absent(
            &suffix_path(&old.state, ".tmp"),
            "the current marker temporary file exists",
        )?;
        ensure_absent(&new.state, "the destination transaction marker exists")?;
        ensure_absent(
            &suffix_path(&new.state, ".tmp"),
            "the destination marker temporary file exists",
        )?;
        ensure_no_recovery_artifacts(&old.executable)?;
        if authority_state == AuthorityState::Absent {
            rewrite_state_authority(self, &old.authority, &old.authority_temp, &old.state)?;
        }
        if old.state != new.state && authority_state != AuthorityState::Migrated {
            rewrite_state_authority(self, &old.authority, &old.authority_temp, &new.state)?;
        }
        Ok(migrated)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum AuthorityState {
    Absent,
    Current,
    Migrated,
}

fn ensure_absent(path: &Path, message: &str) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Err(UpdateError::StateMigrationBlocked {
            path: path.to_path_buf(),
            message: message.into(),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(UpdateError::io(path, error)),
    }
}

#[cfg(test)]
#[path = "transaction_migration_tests.rs"]
mod tests;
