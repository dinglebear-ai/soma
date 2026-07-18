use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::artifacts::{
    exact_artifact_name, validate_marker_backup_metadata, validate_marker_staged_metadata,
};
use super::{TestFailpoint, absolute, suffix_path, sync_parent};
use crate::{Result, UpdateError, Updater};

const MAX_MARKER_BYTES: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum MarkerPhase {
    Prepared,
    Installed,
    RollingBack,
    RolledBack,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Marker {
    pub(super) schema_version: u32,
    pub(super) phase: MarkerPhase,
    pub(super) target: String,
    pub(super) previous: String,
    pub(super) executable: PathBuf,
    pub(super) backup: PathBuf,
    pub(super) staged: PathBuf,
    pub(super) attempts: u32,
    pub(super) sha256: String,
    pub(super) previous_sha256: String,
    pub(super) backup_uid: u32,
}

pub(super) fn preflight_marker_lifecycle(path: &Path, marker: &Marker) -> Result<()> {
    let mut largest = marker.clone();
    largest.phase = MarkerPhase::RollingBack;
    largest.attempts = u32::MAX;
    marker_bytes(path, &largest).map(|_| ())
}

pub(super) fn write_marker(updater: &Updater, path: &Path, marker: &Marker) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;

    let bytes = marker_bytes(path, marker)?;
    let temporary = suffix_path(path, ".tmp");
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|error| UpdateError::io(&temporary, error))?;
        use std::io::Write;
        file.write_all(&bytes)
            .map_err(|error| UpdateError::io(&temporary, error))?;
        file.sync_all()
            .map_err(|error| UpdateError::io(&temporary, error))?;
        updater.maybe_fail(TestFailpoint::AfterMarkerTempSync, &temporary)?;
        std::fs::rename(&temporary, path).map_err(|error| UpdateError::io(path, error))?;
        if marker.phase == MarkerPhase::Prepared
            && (updater.failpoint_active(TestFailpoint::AfterPreparedMarkerRename)
                || updater.failpoint_active(
                    TestFailpoint::AfterPreparedMarkerRenameWithStateCleanupFailure,
                ))
        {
            return Err(UpdateError::io(
                path,
                std::io::Error::other("injected prepared-marker parent sync failure"),
            ));
        }
        sync_parent(path)
    })();
    if result.is_err()
        && temporary.exists()
        && !updater.failpoint_active(TestFailpoint::AfterMarkerTempSync)
    {
        std::fs::remove_file(&temporary).map_err(|error| UpdateError::io(&temporary, error))?;
    }
    result
}

fn marker_bytes(path: &Path, marker: &Marker) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec_pretty(marker).map_err(|error| UpdateError::InvalidMarker {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    if bytes.len() as u64 > MAX_MARKER_BYTES {
        return Err(UpdateError::InvalidMarker {
            path: path.to_path_buf(),
            message: format!("marker exceeds {MAX_MARKER_BYTES} byte limit"),
        });
    }
    Ok(bytes)
}

pub(super) fn cleanup_marker_temp(state: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let temporary = suffix_path(state, ".tmp");
    let metadata = match std::fs::symlink_metadata(&temporary) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(UpdateError::io(&temporary, error)),
    };
    let effective_uid = nix::unistd::geteuid().as_raw();
    if !metadata.file_type().is_file() || !marker_temp_owner_is_valid(metadata.uid(), effective_uid)
    {
        return Err(UpdateError::InvalidMarker {
            path: state.to_path_buf(),
            message: "marker temporary must be an owned non-symlink regular file".into(),
        });
    }
    std::fs::remove_file(&temporary).map_err(|error| UpdateError::io(&temporary, error))?;
    sync_parent(&temporary)
}

pub(super) fn marker_temp_owner_is_valid(owner_uid: u32, effective_uid: u32) -> bool {
    owner_uid == effective_uid
}

fn marker_mode_is_guarded(mode: u32) -> bool {
    mode & 0o7777 == 0o600
}

pub(super) fn read_marker(path: &Path, expected_executable: &Path) -> Result<Option<Marker>> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(UpdateError::io(path, error)),
    };
    let metadata = file
        .metadata()
        .map_err(|error| UpdateError::io(path, error))?;
    if !metadata.file_type().is_file()
        || metadata.uid() != nix::unistd::geteuid().as_raw()
        || !marker_mode_is_guarded(metadata.mode())
    {
        return Err(UpdateError::InvalidMarker {
            path: path.to_path_buf(),
            message: "marker must be a service-owned mode-0600 non-symlink regular file".into(),
        });
    }
    if metadata.len() > MAX_MARKER_BYTES {
        return Err(marker_too_large(path));
    }
    use std::io::Read;
    let mut bytes = Vec::with_capacity(MAX_MARKER_BYTES as usize);
    file.take(MAX_MARKER_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| UpdateError::io(path, error))?;
    if bytes.len() as u64 > MAX_MARKER_BYTES {
        return Err(marker_too_large(path));
    }
    let marker: Marker =
        serde_json::from_slice(&bytes).map_err(|error| UpdateError::InvalidMarker {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    let executable = absolute(expected_executable)?;
    let valid_backup = marker.backup.is_absolute()
        && marker.backup.parent() == executable.parent()
        && exact_artifact_name(&executable, &marker.backup, "rollback", false).is_some();
    let valid_staged = marker.staged.is_absolute()
        && marker.staged.parent() == executable.parent()
        && exact_artifact_name(&executable, &marker.staged, "update", true).is_some();
    if marker.schema_version != 3
        || marker.executable != executable
        || !valid_backup
        || !valid_staged
    {
        return Err(UpdateError::InvalidMarker {
            path: path.to_path_buf(),
            message: "unsupported schema or unsafe recovery path".into(),
        });
    }
    validate_marker_backup_metadata(path, &marker)?;
    validate_marker_staged_metadata(path, &marker)?;
    Ok(Some(marker))
}

fn marker_too_large(path: &Path) -> UpdateError {
    UpdateError::InvalidMarker {
        path: path.to_path_buf(),
        message: format!("marker exceeds {MAX_MARKER_BYTES} byte limit"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_preflight_rejects_marker_that_only_fits_prepared_phase() {
        let state = Path::new("/state/update.json");
        let mut marker = Marker {
            schema_version: 3,
            phase: MarkerPhase::Prepared,
            target: "2.0.0".into(),
            previous: String::new(),
            executable: "/bin/agent".into(),
            backup: "/bin/.agent.rollback-1-1".into(),
            staged: "/bin/.agent.update-1-1.part".into(),
            attempts: 0,
            sha256: "a".repeat(64),
            previous_sha256: "b".repeat(64),
            backup_uid: u32::MAX,
        };
        let base = marker_bytes(state, &marker).unwrap().len();
        marker.previous = "v".repeat(MAX_MARKER_BYTES as usize - base);

        assert_eq!(
            marker_bytes(state, &marker).unwrap().len(),
            MAX_MARKER_BYTES as usize
        );
        assert!(matches!(
            preflight_marker_lifecycle(state, &marker),
            Err(UpdateError::InvalidMarker { .. })
        ));
    }

    #[test]
    fn marker_open_rejects_symlinks_without_reading_the_target() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("foreign-marker");
        let state = temp.path().join("update.json");
        std::fs::write(&target, b"foreign bytes").unwrap();
        std::os::unix::fs::symlink(&target, &state).unwrap();

        assert!(read_marker(&state, &temp.path().join("agent")).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"foreign bytes");
    }

    #[test]
    fn marker_open_rejects_fifo_without_waiting_for_a_writer() {
        use nix::sys::stat::Mode;
        use std::time::{Duration, Instant};

        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("update.json");
        nix::unistd::mkfifo(&state, Mode::S_IRUSR | Mode::S_IWUSR).unwrap();
        let mut delayed_writer = std::process::Command::new("sh")
            .arg("-c")
            .arg("sleep 0.3; printf x > \"$1\"")
            .arg("marker-writer")
            .arg(&state)
            .spawn()
            .unwrap();

        let started = Instant::now();
        let result = read_marker(&state, &temp.path().join("agent"));
        let elapsed = started.elapsed();
        let _ = delayed_writer.kill();
        let _ = delayed_writer.wait();

        assert!(matches!(result, Err(UpdateError::InvalidMarker { .. })));
        assert!(
            elapsed < Duration::from_millis(150),
            "FIFO marker open blocked for {elapsed:?}"
        );
    }

    #[test]
    fn marker_mode_requires_exactly_0600_without_special_bits() {
        assert!(marker_mode_is_guarded(0o600));
        for mode in [0o400, 0o500, 0o700, 0o640, 0o4600] {
            assert!(!marker_mode_is_guarded(mode), "accepted mode {mode:o}");
        }
    }

    #[test]
    fn marker_open_rejects_non_exact_mode_without_repairing_it() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("update.json");
        std::fs::write(&state, b"{}").unwrap();
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(matches!(
            read_marker(&state, &temp.path().join("agent")),
            Err(UpdateError::InvalidMarker { .. })
        ));
        assert_eq!(
            std::fs::metadata(&state).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }
}
