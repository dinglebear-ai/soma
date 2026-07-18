use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::{RecoveryAction, Result, UpdateError, Updater, ValidatedArtifact};

static TRANSACTION_COUNTER: AtomicU64 = AtomicU64::new(0);

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

#[derive(Debug, Deserialize, Serialize)]
struct Marker {
    schema_version: u32,
    target: String,
    previous: String,
    executable: PathBuf,
    backup: PathBuf,
    attempts: u32,
    sha256: String,
}

struct TransactionLock {
    _file: File,
}

impl Updater {
    #[cfg(unix)]
    pub async fn install(
        &self,
        validated: ValidatedArtifact,
        previous_version: impl Into<String>,
    ) -> Result<InstallOutcome> {
        let _lock = self.transaction_lock()?;
        let executable = absolute(self.layout().executable())?;
        let state = absolute(self.layout().state_file())?;
        let previous = previous_version.into();
        let target = validated.target_version().to_owned();
        let backup = unique_backup(&executable);
        create_backup(&executable, &backup)?;
        let marker = Marker {
            schema_version: 1,
            target: target.clone(),
            previous: previous.clone(),
            executable: executable.clone(),
            backup: backup.clone(),
            attempts: 0,
            sha256: validated.sha256().to_owned(),
        };
        if let Err(error) = write_marker(&state, &marker) {
            remove_file(&backup)?;
            return Err(error);
        }
        if let Err(source) = std::fs::rename(validated.path(), &executable) {
            remove_file(&state)?;
            remove_file(&backup)?;
            return Err(UpdateError::io(&executable, source));
        }
        sync_parent(&executable)?;
        Ok(InstallOutcome::RestartRequired {
            executable,
            from: previous,
            to: target,
        })
    }

    #[cfg(not(unix))]
    pub async fn install(
        &self,
        _validated: ValidatedArtifact,
        _previous_version: impl Into<String>,
    ) -> Result<InstallOutcome> {
        Err(UpdateError::UnsupportedPlatform)
    }

    pub async fn recover_on_startup(&self, running_version: &str) -> Result<RecoveryAction> {
        #[cfg(not(unix))]
        {
            let _ = running_version;
            return Err(UpdateError::UnsupportedPlatform);
        }
        #[cfg(unix)]
        {
            let _lock = self.transaction_lock()?;
            let state = absolute(self.layout().state_file())?;
            let Some(mut marker) = read_marker(&state, self.layout().executable())? else {
                return Ok(RecoveryAction::NoPendingUpdate);
            };
            if marker.target != running_version {
                if marker.backup.exists() {
                    remove_file(&marker.backup)?;
                }
                remove_file(&state)?;
                sync_parent(&state)?;
                return Ok(RecoveryAction::StaleMarkerRemoved {
                    target: marker.target,
                    running: running_version.to_owned(),
                });
            }
            marker.attempts = marker.attempts.saturating_add(1);
            if marker.attempts <= self.policy().max_unconfirmed_restarts() {
                write_marker(&state, &marker)?;
                return Ok(RecoveryAction::PendingUpdate {
                    target: marker.target,
                    attempts: marker.attempts,
                    max_attempts: self.policy().max_unconfirmed_restarts(),
                });
            }
            if !marker.backup.is_file() {
                return Err(UpdateError::MissingRollback {
                    path: marker.backup,
                });
            }
            std::fs::rename(&marker.backup, &marker.executable)
                .map_err(|error| UpdateError::io(&marker.executable, error))?;
            sync_parent(&marker.executable)?;
            remove_file(&state)?;
            sync_parent(&state)?;
            Ok(RecoveryAction::RollbackInstalled {
                executable: marker.executable,
                restored_version: marker.previous,
            })
        }
    }

    pub async fn confirm_success(&self, running_version: &str) -> Result<ConfirmationOutcome> {
        #[cfg(not(unix))]
        {
            let _ = running_version;
            return Err(UpdateError::UnsupportedPlatform);
        }
        #[cfg(unix)]
        {
            let _lock = self.transaction_lock()?;
            let state = absolute(self.layout().state_file())?;
            let Some(marker) = read_marker(&state, self.layout().executable())? else {
                return Ok(ConfirmationOutcome::NoPendingUpdate);
            };
            if marker.target != running_version {
                return Err(UpdateError::RunningVersionMismatch {
                    running: running_version.to_owned(),
                    target: marker.target,
                });
            }
            if !marker.backup.is_file() {
                return Err(UpdateError::MissingRollback {
                    path: marker.backup,
                });
            }
            remove_file(&marker.backup)?;
            remove_file(&state)?;
            sync_parent(&state)?;
            Ok(ConfirmationOutcome::Confirmed {
                version: running_version.to_owned(),
            })
        }
    }

    fn transaction_lock(&self) -> Result<TransactionLock> {
        let state = absolute(self.layout().state_file())?;
        let lock_path = suffix_path(&state, ".lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|error| UpdateError::io(&lock_path, error))?;
        file.try_lock_exclusive().map_err(|error| {
            if error.kind() == std::io::ErrorKind::WouldBlock {
                UpdateError::UpdateInProgress {
                    path: lock_path.clone(),
                }
            } else {
                UpdateError::io(&lock_path, error)
            }
        })?;
        Ok(TransactionLock { _file: file })
    }
}

fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|directory| directory.join(path))
        .map_err(|error| UpdateError::io(path, error))
}

fn suffix_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn unique_backup(executable: &Path) -> PathBuf {
    let name = executable
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("executable");
    executable.with_file_name(format!(
        ".{name}.rollback-{}-{}",
        std::process::id(),
        TRANSACTION_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn create_backup(executable: &Path, backup: &Path) -> Result<()> {
    match std::fs::hard_link(executable, backup) {
        Ok(()) => {}
        Err(_) => {
            let mut source =
                File::open(executable).map_err(|error| UpdateError::io(executable, error))?;
            let mut destination = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(backup)
                .map_err(|error| UpdateError::io(backup, error))?;
            std::io::copy(&mut source, &mut destination)
                .map_err(|error| UpdateError::io(backup, error))?;
            destination
                .sync_all()
                .map_err(|error| UpdateError::io(backup, error))?;
        }
    }
    File::open(backup)
        .and_then(|file| file.sync_all())
        .map_err(|error| UpdateError::io(backup, error))
}

fn write_marker(path: &Path, marker: &Marker) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(marker).map_err(|error| UpdateError::InvalidMarker {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let temporary = suffix_path(
        path,
        &format!(
            ".tmp-{}-{}",
            std::process::id(),
            TRANSACTION_COUNTER.fetch_add(1, Ordering::Relaxed)
        ),
    );
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| UpdateError::io(&temporary, error))?;
        use std::io::Write;
        file.write_all(&bytes)
            .map_err(|error| UpdateError::io(&temporary, error))?;
        file.sync_all()
            .map_err(|error| UpdateError::io(&temporary, error))?;
        std::fs::rename(&temporary, path).map_err(|error| UpdateError::io(path, error))?;
        sync_parent(path)
    })();
    if result.is_err() && temporary.exists() {
        std::fs::remove_file(&temporary).map_err(|error| UpdateError::io(&temporary, error))?;
    }
    result
}

fn read_marker(path: &Path, expected_executable: &Path) -> Result<Option<Marker>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(UpdateError::io(path, error)),
    };
    let marker: Marker =
        serde_json::from_slice(&bytes).map_err(|error| UpdateError::InvalidMarker {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    let executable = absolute(expected_executable)?;
    let valid_backup = marker.backup.is_absolute()
        && marker.backup.parent() == executable.parent()
        && marker
            .backup
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".") && name.contains(".rollback-"));
    if marker.schema_version != 1 || marker.executable != executable || !valid_backup {
        return Err(UpdateError::InvalidMarker {
            path: path.to_path_buf(),
            message: "unsupported schema or unsafe recovery path".into(),
        });
    }
    Ok(Some(marker))
}

fn remove_file(path: &Path) -> Result<()> {
    std::fs::remove_file(path).map_err(|error| UpdateError::io(path, error))
}

fn sync_parent(path: &Path) -> Result<()> {
    let parent = path.parent().ok_or(UpdateError::InvalidPolicy(
        "transaction path must have a parent",
    ))?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| UpdateError::io(parent, error))
}
