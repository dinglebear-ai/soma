use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use fs2::FileExt;

use super::transaction_io::{path_identity, suffix_path};
use crate::{Result, UpdateError, Updater, reject_executable_leaf_symlink};

pub(super) struct TransactionLock {
    _file: File,
}

pub(super) struct LayoutPaths {
    pub(super) executable: PathBuf,
    pub(super) state: PathBuf,
    pub(super) lock: PathBuf,
}

impl Updater {
    pub(super) fn transaction_lock(&self, lock_path: &Path) -> Result<TransactionLock> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(|error| UpdateError::io(lock_path, error))?;
        file.try_lock_exclusive().map_err(|error| {
            if error.kind() == std::io::ErrorKind::WouldBlock {
                UpdateError::UpdateInProgress {
                    path: lock_path.to_path_buf(),
                }
            } else {
                UpdateError::io(lock_path, error)
            }
        })?;
        Ok(TransactionLock { _file: file })
    }

    pub(super) fn validated_layout(&self) -> Result<LayoutPaths> {
        reject_executable_leaf_symlink(self.layout().executable())?;
        let executable = path_identity(self.layout().executable())?;
        let state = path_identity(self.layout().state_file())?;
        let lock = suffix_path(&state, ".lock");
        for (first, second) in [(&executable, &state), (&executable, &lock), (&state, &lock)] {
            if first == second {
                return Err(UpdateError::InvalidLayout {
                    first: first.clone(),
                    second: second.clone(),
                });
            }
        }
        Ok(LayoutPaths {
            executable,
            state,
            lock,
        })
    }
}
