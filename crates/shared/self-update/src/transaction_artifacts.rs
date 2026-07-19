use std::path::Path;

use super::{Marker, MarkerPhase, hash_file, path_validation::paths_may_alias, sync_parent};
use crate::{Result, UpdateError};

pub(super) fn validate_backup_candidate(
    executable: &Path,
    state: &Path,
    locks: &[std::path::PathBuf],
    marker_temp: &Path,
    staged: &Path,
    backup: &Path,
) -> Result<()> {
    for protected in std::iter::once(executable)
        .chain(std::iter::once(state))
        .chain(locks.iter().map(std::path::PathBuf::as_path))
        .chain([marker_temp, staged])
    {
        if paths_may_alias(backup, protected) {
            return Err(UpdateError::InvalidLayout {
                first: backup.to_path_buf(),
                second: protected.to_path_buf(),
            });
        }
    }
    Ok(())
}

pub(super) fn exact_artifact_name(
    executable: &Path,
    candidate: &Path,
    kind: &str,
    part_suffix: bool,
) -> Option<u32> {
    let executable_name = executable.file_name()?.to_str()?;
    let candidate_name = candidate.file_name()?.to_str()?;
    let prefix = format!(".{executable_name}.{kind}-");
    let remainder = candidate_name.strip_prefix(&prefix)?;
    let remainder = if part_suffix {
        remainder.strip_suffix(".part")?
    } else {
        remainder
    };
    let (pid, counter) = remainder.split_once('-')?;
    if pid.is_empty()
        || counter.is_empty()
        || !pid.bytes().all(|byte| byte.is_ascii_digit())
        || !counter.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    pid.parse().ok()
}

pub(super) fn validate_marker_backup_metadata(state: &Path, marker: &Marker) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let metadata = match std::fs::symlink_metadata(&marker.backup) {
        Ok(metadata) => metadata,
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && matches!(
                    marker.phase,
                    MarkerPhase::RollingBack | MarkerPhase::RolledBack
                ) =>
        {
            return Ok(());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(UpdateError::MissingRollback {
                path: marker.backup.clone(),
            });
        }
        Err(error) => return Err(UpdateError::io(&marker.backup, error)),
    };
    if !metadata.file_type().is_file()
        || !backup_owner_matches_recorded(metadata.uid(), marker.backup_uid)
    {
        return Err(UpdateError::InvalidMarker {
            path: state.to_path_buf(),
            message: "rollback backup must be an owned non-symlink regular file".into(),
        });
    }
    Ok(())
}

fn backup_owner_matches_recorded(actual_uid: u32, recorded_uid: u32) -> bool {
    actual_uid == recorded_uid
}

pub(super) fn validate_marker_staged_metadata(state: &Path, marker: &Marker) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let metadata = match std::fs::symlink_metadata(&marker.staged) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(UpdateError::io(&marker.staged, error)),
    };
    let expected_uid = nix::unistd::geteuid().as_raw();
    if !metadata.file_type().is_file() || metadata.uid() != expected_uid {
        return Err(UpdateError::InvalidMarker {
            path: state.to_path_buf(),
            message: "staged artifact must be an owned non-symlink regular file".into(),
        });
    }
    Ok(())
}

pub(super) fn validate_rollback_backup(state: &Path, marker: &Marker) -> Result<()> {
    validate_marker_backup_metadata(state, marker)?;
    let actual = hash_file(&marker.backup)?;
    if actual != marker.previous_sha256 {
        return Err(UpdateError::InvalidMarker {
            path: state.to_path_buf(),
            message: "rollback backup digest does not match previous executable".into(),
        });
    }
    Ok(())
}

pub(super) fn cleanup_owned_artifacts(
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
        if protected_backup.is_some_and(|protected| same_existing_identity(protected, &path))
            || protected_staging.is_some_and(|protected| same_existing_identity(protected, &path))
        {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let owner_pid = if name.starts_with(&staging_prefix) {
            exact_artifact_name(executable, &path, "update", true)
        } else if name.starts_with(&backup_prefix) {
            exact_artifact_name(executable, &path, "rollback", false)
        } else {
            None
        };
        let Some(owner_pid) = owner_pid else {
            continue;
        };
        if process_is_alive(owner_pid) {
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

pub(super) fn ensure_no_recovery_artifacts(executable: &Path) -> Result<()> {
    let directory = executable.parent().ok_or(UpdateError::InvalidPolicy(
        "executable must have a parent directory",
    ))?;
    for entry in std::fs::read_dir(directory).map_err(|error| UpdateError::io(directory, error))? {
        let entry = entry.map_err(|error| UpdateError::io(directory, error))?;
        let path = entry.path();
        if exact_artifact_name(executable, &path, "update", true).is_some()
            || exact_artifact_name(executable, &path, "rollback", false).is_some()
        {
            return Err(UpdateError::StateMigrationBlocked {
                path,
                message: "a staged or rollback recovery artifact exists".into(),
            });
        }
    }
    Ok(())
}

fn same_existing_identity(first: &Path, second: &Path) -> bool {
    match (std::fs::canonicalize(first), std::fs::canonicalize(second)) {
        (Ok(first), Ok(second)) => first == second,
        _ => first == second,
    }
}

fn process_is_alive(pid: u32) -> bool {
    use nix::errno::Errno;
    use nix::sys::signal::kill;
    use nix::unistd::Pid;

    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    match kill(Pid::from_raw(pid), None) {
        Ok(()) | Err(Errno::EPERM) => true,
        Err(Errno::ESRCH) => false,
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::backup_owner_matches_recorded;

    #[test]
    fn backup_owner_validation_uses_recorded_owner_not_installed_owner() {
        let recorded_pre_update_owner = 0;
        let newly_installed_owner = 1_000;

        assert_ne!(recorded_pre_update_owner, newly_installed_owner);
        assert!(backup_owner_matches_recorded(
            recorded_pre_update_owner,
            recorded_pre_update_owner
        ));
        assert!(!backup_owner_matches_recorded(
            newly_installed_owner,
            recorded_pre_update_owner
        ));
    }
}
