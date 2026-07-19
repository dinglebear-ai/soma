use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use fs2::FileExt;

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

pub(super) struct LayoutPaths {
    pub(super) executable: PathBuf,
    pub(super) state: PathBuf,
    pub(super) locks: Vec<PathBuf>,
    executable_lock: PathBuf,
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
        if metadata.mode() & 0o777 != 0o600 {
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|error| UpdateError::io(lock_path, error))?;
            file.sync_all()
                .map_err(|error| UpdateError::io(lock_path, error))?;
            let repaired = file
                .metadata()
                .map_err(|error| UpdateError::io(lock_path, error))?;
            if repaired.mode() & 0o777 != 0o600 {
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
        let mut locks: Vec<_> = paths
            .locks
            .iter()
            .map(|path| self.transaction_lock(path))
            .collect::<Result<_>>()?;
        let executable_lock = locks
            .iter_mut()
            .find(|lock| lock.path == paths.executable_lock)
            .ok_or(UpdateError::InvalidPolicy(
                "executable transaction lock is missing",
            ))?;
        bind_executable_state(executable_lock, &paths.state)?;
        Ok(locks)
    }

    pub(super) fn validated_layout(&self) -> Result<LayoutPaths> {
        self.ensure_layout_bound()?;
        reject_executable_leaf_symlink(self.layout().executable())?;
        let executable = path_identity(self.layout().executable())?;
        let state = bind_state_identity(self.layout().state_file())
            .map_err(|error| UpdateError::io(self.layout().state_file(), error))?;
        let executable_lock = executable_lock_path(&executable)?;
        let mut locks = vec![executable_lock.clone(), suffix_path(&state, ".lock")];
        locks.sort();
        locks.dedup();
        for (first, second) in std::iter::once((&executable, &state)).chain(
            locks
                .iter()
                .flat_map(|lock| [(&executable, lock), (&state, lock)]),
        ) {
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
            locks,
            executable_lock,
        })
    }
}

fn bind_executable_state(lock: &mut TransactionLock, state: &Path) -> Result<()> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    const MAX_BINDING_BYTES: u64 = 16 * 1024;
    let length = lock
        .file
        .metadata()
        .map_err(|error| UpdateError::io(&lock.path, error))?
        .len();
    if length > MAX_BINDING_BYTES {
        return Err(UpdateError::InvalidMarker {
            path: lock.path.clone(),
            message: "executable lock state binding is too large".into(),
        });
    }
    lock.file
        .rewind()
        .map_err(|error| UpdateError::io(&lock.path, error))?;
    let mut existing = Vec::with_capacity(length as usize);
    lock.file
        .read_to_end(&mut existing)
        .map_err(|error| UpdateError::io(&lock.path, error))?;
    if existing.is_empty() {
        let bytes = state.as_os_str().as_bytes();
        if bytes.len() as u64 > MAX_BINDING_BYTES {
            return Err(UpdateError::InvalidPolicy("state path is too long"));
        }
        lock.file
            .write_all(bytes)
            .map_err(|error| UpdateError::io(&lock.path, error))?;
        lock.file
            .sync_all()
            .map_err(|error| UpdateError::io(&lock.path, error))?;
        super::transaction_io::sync_parent(&lock.path)?;
        return Ok(());
    }
    let bound = PathBuf::from(std::ffi::OsString::from_vec(existing));
    if bound != state {
        return Err(UpdateError::InvalidLayout {
            first: bound,
            second: state.to_path_buf(),
        });
    }
    Ok(())
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
