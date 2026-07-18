use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{BackupStrategy, RecoveryAction, Result, UpdateError, Updater, ValidatedArtifact};

static TRANSACTION_COUNTER: AtomicU64 = AtomicU64::new(0);
const MAX_MARKER_BYTES: u64 = 64 * 1024;

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

struct LayoutPaths {
    executable: PathBuf,
    state: PathBuf,
    lock: PathBuf,
}

impl Updater {
    pub async fn install(
        &self,
        validated: ValidatedArtifact,
        previous_version: impl Into<String>,
    ) -> Result<InstallOutcome> {
        let paths = self.validated_layout()?;
        let _lock = self.transaction_lock(&paths.lock)?;
        let executable = paths.executable;
        let state = paths.state;
        let validated_path = absolute(validated.path())?;
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
        let actual_digest = hash_file(&validated_path)?;
        if actual_digest != validated.sha256() {
            return Err(UpdateError::DigestMismatch {
                expected: validated.sha256().to_owned(),
                actual: actual_digest,
            });
        }
        let previous = previous_version.into();
        let target = validated.target_version().to_owned();
        let backup = unique_backup(&executable);
        create_backup(&executable, &backup, self.policy().backup_strategy())?;
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
        if let Err(source) = std::fs::rename(&validated_path, &executable) {
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

    pub async fn recover_on_startup(&self, running_version: &str) -> Result<RecoveryAction> {
        let paths = self.validated_layout()?;
        let _lock = self.transaction_lock(&paths.lock)?;
        let state = paths.state;
        let marker = read_marker(&state, &paths.executable)?;
        cleanup_owned_artifacts(
            &paths.executable,
            marker.as_ref().map(|marker| marker.backup.as_path()),
            None,
        )?;
        let Some(mut marker) = marker else {
            return Ok(RecoveryAction::NoPendingUpdate);
        };
        if marker.target != running_version {
            return Err(UpdateError::RunningVersionMismatch {
                running: running_version.to_owned(),
                target: marker.target,
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

    pub async fn confirm_success(&self, running_version: &str) -> Result<ConfirmationOutcome> {
        let paths = self.validated_layout()?;
        let _lock = self.transaction_lock(&paths.lock)?;
        let state = paths.state;
        let marker = read_marker(&state, &paths.executable)?;
        cleanup_owned_artifacts(
            &paths.executable,
            marker.as_ref().map(|marker| marker.backup.as_path()),
            None,
        )?;
        let Some(marker) = marker else {
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
        remove_file(&state)?;
        sync_parent(&state)?;
        remove_file(&marker.backup)?;
        sync_parent(&marker.backup)?;
        Ok(ConfirmationOutcome::Confirmed {
            version: running_version.to_owned(),
        })
    }

    fn transaction_lock(&self, lock_path: &Path) -> Result<TransactionLock> {
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

    fn validated_layout(&self) -> Result<LayoutPaths> {
        let executable = path_identity(self.layout().executable())?;
        let state = path_identity(self.layout().state_file())?;
        let lock = path_identity(&suffix_path(self.layout().state_file(), ".lock"))?;
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

fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|directory| directory.join(path))
        .map_err(|error| UpdateError::io(path, error))
}

fn path_identity(path: &Path) -> Result<PathBuf> {
    let absolute = absolute(path)?;
    match std::fs::canonicalize(&absolute) {
        Ok(canonical) => Ok(canonical),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(absolute),
        Err(error) => Err(UpdateError::io(&absolute, error)),
    }
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

fn create_backup(executable: &Path, backup: &Path, strategy: BackupStrategy) -> Result<()> {
    let hard_linked = strategy == BackupStrategy::HardLinkOrCopy
        && std::fs::hard_link(executable, backup).is_ok();
    if !hard_linked {
        let mut source =
            File::open(executable).map_err(|error| UpdateError::io(executable, error))?;
        let source_permissions = source
            .metadata()
            .map_err(|error| UpdateError::io(executable, error))?
            .permissions();
        let mut destination = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(backup)
            .map_err(|error| UpdateError::io(backup, error))?;
        std::io::copy(&mut source, &mut destination)
            .map_err(|error| UpdateError::io(backup, error))?;
        destination
            .set_permissions(source_permissions)
            .map_err(|error| UpdateError::io(backup, error))?;
        destination
            .sync_all()
            .map_err(|error| UpdateError::io(backup, error))?;
    }
    let synced = File::open(backup)
        .and_then(|file| file.sync_all())
        .map_err(|error| UpdateError::io(backup, error))
        .and_then(|()| sync_parent(backup));
    if let Err(error) = synced {
        std::fs::remove_file(backup).map_err(|cleanup| UpdateError::io(backup, cleanup))?;
        return Err(error);
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|error| UpdateError::io(path, error))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        use std::io::Read;
        let read = file
            .read(&mut buffer)
            .map_err(|error| UpdateError::io(path, error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
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
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(UpdateError::io(path, error)),
    };
    if file
        .metadata()
        .map_err(|error| UpdateError::io(path, error))?
        .len()
        > MAX_MARKER_BYTES
    {
        return Err(UpdateError::InvalidMarker {
            path: path.to_path_buf(),
            message: format!("marker exceeds {MAX_MARKER_BYTES} byte limit"),
        });
    }
    use std::io::Read;
    let mut bytes = Vec::with_capacity(MAX_MARKER_BYTES as usize);
    file.take(MAX_MARKER_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| UpdateError::io(path, error))?;
    if bytes.len() as u64 > MAX_MARKER_BYTES {
        return Err(UpdateError::InvalidMarker {
            path: path.to_path_buf(),
            message: format!("marker exceeds {MAX_MARKER_BYTES} byte limit"),
        });
    }
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

fn cleanup_owned_artifacts(
    executable: &Path,
    protected_backup: Option<&Path>,
    protected_staging: Option<&Path>,
) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let directory = executable.parent().ok_or(UpdateError::InvalidPolicy(
        "executable must have a parent directory",
    ))?;
    let expected_uid = std::fs::metadata(executable)
        .or_else(|_| std::fs::metadata(directory))
        .map_err(|error| UpdateError::io(directory, error))?
        .uid();
    let executable_name = executable
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(UpdateError::InvalidPolicy(
            "executable name must be valid UTF-8",
        ))?;
    let staging_prefix = format!(".{executable_name}.update-");
    let backup_prefix = format!(".{executable_name}.rollback-");
    let mut removed = false;
    for entry in std::fs::read_dir(directory).map_err(|error| UpdateError::io(directory, error))? {
        let entry = entry.map_err(|error| UpdateError::io(directory, error))?;
        let path = entry.path();
        if protected_backup.is_some_and(|protected| protected == path)
            || protected_staging.is_some_and(|protected| protected == path)
        {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let owned_name = (name.starts_with(&staging_prefix) && name.ends_with(".part"))
            || name.starts_with(&backup_prefix);
        if !owned_name {
            continue;
        }
        let metadata =
            std::fs::symlink_metadata(&path).map_err(|error| UpdateError::io(&path, error))?;
        if !metadata.file_type().is_file() || metadata.uid() != expected_uid {
            continue;
        }
        std::fs::remove_file(&path).map_err(|error| UpdateError::io(&path, error))?;
        removed = true;
    }
    if removed {
        sync_parent(executable)?;
    }
    Ok(())
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
