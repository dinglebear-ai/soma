use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use fs2::FileExt;

use super::authority::{authority_paths, ensure_state_authority};
use super::path_validation::validate_distinct_paths;
use super::transaction_io::{path_identity, suffix_path};
use crate::{Result, UpdateError, Updater, bind_state_identity, reject_executable_leaf_symlink};

pub(super) struct TransactionLock {
    file: File,
    path: PathBuf,
}

impl Drop for TransactionLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

pub(crate) struct LayoutPaths {
    pub(crate) executable: PathBuf,
    pub(super) state: PathBuf,
    pub(super) locks: Vec<PathBuf>,
    pub(crate) protected: Vec<PathBuf>,
    pub(super) executable_lock: PathBuf,
    pub(super) authority: PathBuf,
    pub(super) authority_temp: PathBuf,
}

impl Updater {
    fn transaction_lock(&self, lock_path: &Path) -> Result<TransactionLock> {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
            .open(lock_path)
            .map_err(|error| UpdateError::io(lock_path, error))?;
        let metadata = file
            .metadata()
            .map_err(|error| UpdateError::io(lock_path, error))?;
        if !metadata.file_type().is_file() || metadata.uid() != nix::unistd::geteuid().as_raw() {
            return Err(UpdateError::InvalidMarker {
                path: lock_path.to_path_buf(),
                message: "transaction lock must be a service-owned non-symlink regular file".into(),
            });
        }
        if metadata.mode() & 0o7777 != 0o600 {
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|error| UpdateError::io(lock_path, error))?;
            file.sync_all()
                .map_err(|error| UpdateError::io(lock_path, error))?;
            let repaired = file
                .metadata()
                .map_err(|error| UpdateError::io(lock_path, error))?;
            if repaired.mode() & 0o7777 != 0o600 {
                return Err(UpdateError::InvalidMarker {
                    path: lock_path.to_path_buf(),
                    message: "transaction lock permissions must be 0600".into(),
                });
            }
        }
        file.try_lock_exclusive().map_err(|error| {
            if error.kind() == std::io::ErrorKind::WouldBlock {
                UpdateError::UpdateInProgress {
                    path: lock_path.to_path_buf(),
                }
            } else {
                UpdateError::io(lock_path, error)
            }
        })?;
        Ok(TransactionLock {
            file,
            path: lock_path.to_path_buf(),
        })
    }

    pub(super) fn transaction_locks(&self, paths: &LayoutPaths) -> Result<Vec<TransactionLock>> {
        let locks = self.acquire_transaction_locks(&paths.locks)?;
        if !locks.iter().any(|lock| lock.path == paths.executable_lock) {
            return Err(UpdateError::InvalidPolicy(
                "executable transaction lock is missing",
            ));
        }
        ensure_state_authority(self, &paths.authority, &paths.authority_temp, &paths.state)?;
        Ok(locks)
    }

    pub(super) fn acquire_transaction_locks(
        &self,
        lock_paths: &[PathBuf],
    ) -> Result<Vec<TransactionLock>> {
        lock_paths
            .iter()
            .map(|path| self.transaction_lock(path))
            .collect()
    }

    pub(crate) fn validated_layout(&self) -> Result<LayoutPaths> {
        self.ensure_layout_bound()?;
        reject_executable_leaf_symlink(self.layout().executable())?;
        let executable = path_identity(self.layout().executable())?;
        let state = bind_state_identity(self.layout().state_file())
            .map_err(|error| UpdateError::io(self.layout().state_file(), error))?;
        let executable_lock = executable_lock_path(&executable)?;
        let (authority, authority_temp) = authority_paths(&executable)?;
        let state_temp = suffix_path(&state, ".tmp");
        let mut locks = vec![executable_lock.clone(), suffix_path(&state, ".lock")];
        locks.sort();
        locks.dedup();
        let mut protected = vec![
            executable.clone(),
            state.clone(),
            state_temp,
            authority.clone(),
            authority_temp.clone(),
        ];
        protected.extend(locks.iter().cloned());
        validate_distinct_paths(&protected)?;
        Ok(LayoutPaths {
            executable,
            state,
            locks,
            protected,
            executable_lock,
            authority,
            authority_temp,
        })
    }
}

pub(super) fn executable_lock_path(executable: &Path) -> Result<PathBuf> {
    let name = executable
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(UpdateError::InvalidPolicy(
            "executable name must be valid UTF-8",
        ))?;
    Ok(executable.with_file_name(format!(".{name}.update.lock")))
}
