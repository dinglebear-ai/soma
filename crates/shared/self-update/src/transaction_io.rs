use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use crate::validation::ArtifactIdentity;
use crate::{BackupStrategy, Result, UpdateError, ValidatedArtifact};

static TRANSACTION_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|directory| directory.join(path))
        .map_err(|error| UpdateError::io(path, error))
}

pub(super) fn path_identity(path: &Path) -> Result<PathBuf> {
    path_identity_inner(path, 0)
}

fn path_identity_inner(path: &Path, depth: usize) -> Result<PathBuf> {
    if depth > 8 {
        return Err(UpdateError::InvalidPolicy(
            "transaction path has too many symlink indirections",
        ));
    }
    let absolute = absolute(path)?;
    match std::fs::symlink_metadata(&absolute) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let target =
                std::fs::read_link(&absolute).map_err(|error| UpdateError::io(&absolute, error))?;
            let target = if target.is_absolute() {
                target
            } else {
                absolute
                    .parent()
                    .ok_or(UpdateError::InvalidPolicy(
                        "transaction path must have a parent",
                    ))?
                    .join(target)
            };
            return path_identity_inner(&target, depth + 1);
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(UpdateError::io(&absolute, error)),
    }
    match std::fs::canonicalize(&absolute) {
        Ok(canonical) => Ok(canonical),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = absolute.parent().ok_or(UpdateError::InvalidPolicy(
                "transaction path must have a parent",
            ))?;
            let canonical_parent = std::fs::canonicalize(parent)
                .map_err(|parent_error| UpdateError::io(parent, parent_error))?;
            Ok(
                canonical_parent.join(absolute.file_name().ok_or(UpdateError::InvalidPolicy(
                    "transaction path must have a file name",
                ))?),
            )
        }
        Err(error) => Err(UpdateError::io(&absolute, error)),
    }
}

pub(super) fn suffix_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

pub(super) fn unique_backup(executable: &Path) -> PathBuf {
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

pub(super) fn create_backup(
    executable: &Path,
    backup: &Path,
    strategy: BackupStrategy,
) -> Result<()> {
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

pub(super) fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|error| UpdateError::io(path, error))?;
    hash_reader(&mut file, path)
}

pub(super) fn hash_stable_validated_artifact(
    validated: &ValidatedArtifact,
    path: &Path,
) -> Result<String> {
    let path_metadata =
        std::fs::symlink_metadata(path).map_err(|error| UpdateError::io(path, error))?;
    if !path_metadata.file_type().is_file()
        || ArtifactIdentity::from_metadata(&path_metadata) != validated.identity
    {
        return Err(UpdateError::ArtifactIdentityChanged {
            path: path.to_path_buf(),
        });
    }
    let mut file = File::open(path).map_err(|error| UpdateError::io(path, error))?;
    let opened_identity = ArtifactIdentity::from_metadata(
        &file
            .metadata()
            .map_err(|error| UpdateError::io(path, error))?,
    );
    if opened_identity != validated.identity {
        return Err(UpdateError::ArtifactIdentityChanged {
            path: path.to_path_buf(),
        });
    }
    let digest = hash_reader(&mut file, path)?;
    let after_read_identity = ArtifactIdentity::from_metadata(
        &file
            .metadata()
            .map_err(|error| UpdateError::io(path, error))?,
    );
    let final_path_metadata =
        std::fs::symlink_metadata(path).map_err(|error| UpdateError::io(path, error))?;
    if !final_path_metadata.file_type().is_file()
        || after_read_identity != validated.identity
        || ArtifactIdentity::from_metadata(&final_path_metadata) != validated.identity
    {
        return Err(UpdateError::ArtifactIdentityChanged {
            path: path.to_path_buf(),
        });
    }
    Ok(digest)
}

fn hash_reader(file: &mut File, path: &Path) -> Result<String> {
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

pub(super) fn remove_if_present_and_sync(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => sync_parent(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(UpdateError::io(path, error)),
    }
}

pub(super) fn remove_and_sync(path: &Path) -> Result<()> {
    remove_file(path)?;
    sync_parent(path)
}

pub(super) fn remove_file(path: &Path) -> Result<()> {
    std::fs::remove_file(path).map_err(|error| UpdateError::io(path, error))
}

pub(super) fn sync_parent(path: &Path) -> Result<()> {
    let parent = path.parent().ok_or(UpdateError::InvalidPolicy(
        "transaction path must have a parent",
    ))?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| UpdateError::io(parent, error))
}
