use std::fs::{File, OpenOptions};
use std::io::Read;
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
) -> Result<u32> {
    let hard_linked = strategy == BackupStrategy::HardLinkOrCopy
        && std::fs::hard_link(executable, backup).is_ok();
    if !hard_linked {
        let mut source =
            File::open(executable).map_err(|error| UpdateError::io(executable, error))?;
        let source_permissions = source
            .metadata()
            .map_err(|error| UpdateError::io(executable, error))?
            .permissions();
        write_backup_copy(&mut source, backup, source_permissions)?;
    }
    verify_or_cleanup_created_backup(backup, verify_created_backup, remove_and_sync)
}

fn verify_created_backup(backup: &Path) -> Result<u32> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    let file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(backup)
        .map_err(|error| UpdateError::io(backup, error))?;
    let metadata = file
        .metadata()
        .map_err(|error| UpdateError::io(backup, error))?;
    if !metadata.file_type().is_file() {
        return Err(UpdateError::InvalidMarker {
            path: backup.to_path_buf(),
            message: "rollback backup must be a non-symlink regular file".into(),
        });
    }
    file.sync_all()
        .map_err(|error| UpdateError::io(backup, error))?;
    sync_parent(backup)?;
    Ok(metadata.uid())
}

fn verify_or_cleanup_created_backup<
    V: FnOnce(&Path) -> Result<u32>,
    C: FnOnce(&Path) -> Result<()>,
>(
    backup: &Path,
    verify: V,
    cleanup: C,
) -> Result<u32> {
    match verify(backup) {
        Ok(uid) => Ok(uid),
        Err(operation) => match cleanup(backup) {
            Ok(()) => Err(operation),
            Err(cleanup) => Err(UpdateError::TransactionCleanupFailed {
                operation: Box::new(operation),
                cleanup: Box::new(cleanup),
            }),
        },
    }
}

fn write_backup_copy<R: Read>(
    source: &mut R,
    backup: &Path,
    source_permissions: std::fs::Permissions,
) -> Result<()> {
    write_backup_copy_with_cleanup(source, backup, source_permissions, remove_and_sync)
}

fn write_backup_copy_with_cleanup<R: Read, C: FnOnce(&Path) -> Result<()>>(
    source: &mut R,
    backup: &Path,
    source_permissions: std::fs::Permissions,
    cleanup: C,
) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut destination = open_backup_copy_destination(backup, source_permissions.mode())?;
    let operation = (|| {
        std::io::copy(source, &mut destination).map_err(|error| UpdateError::io(backup, error))?;
        destination
            .set_permissions(source_permissions)
            .map_err(|error| UpdateError::io(backup, error))?;
        destination
            .sync_all()
            .map_err(|error| UpdateError::io(backup, error))
    })();
    if let Err(operation) = operation {
        drop(destination);
        return match cleanup(backup) {
            Ok(()) => Err(operation),
            Err(cleanup) => Err(UpdateError::TransactionCleanupFailed {
                operation: Box::new(operation),
                cleanup: Box::new(cleanup),
            }),
        };
    }
    Ok(())
}

fn open_backup_copy_destination(backup: &Path, source_mode: u32) -> Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(source_mode & 0o7777)
        .open(backup)
        .map_err(|error| UpdateError::io(backup, error))
}

pub(super) fn restore_validated_artifact_mode(
    validated: &ValidatedArtifact,
    path: &Path,
) -> Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let file = OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| UpdateError::io(path, error))?;
    let metadata = file
        .metadata()
        .map_err(|error| UpdateError::io(path, error))?;
    if !metadata.file_type().is_file()
        || ArtifactIdentity::from_metadata(&metadata) != validated.identity
    {
        return Err(UpdateError::ArtifactIdentityChanged {
            path: path.to_path_buf(),
        });
    }
    file.set_permissions(std::fs::Permissions::from_mode(validated.intended_mode()))
        .map_err(|error| UpdateError::io(path, error))?;
    let repaired = file
        .metadata()
        .map_err(|error| UpdateError::io(path, error))?;
    if ArtifactIdentity::from_metadata(&repaired) != validated.identity
        || repaired.permissions().mode() & 0o7777 != validated.intended_mode()
    {
        return Err(UpdateError::ArtifactIdentityChanged {
            path: path.to_path_buf(),
        });
    }
    file.sync_all()
        .map_err(|error| UpdateError::io(path, error))?;
    ensure_validated_artifact_mode(validated, path)
}

pub(super) fn ensure_validated_artifact_mode(
    validated: &ValidatedArtifact,
    path: &Path,
) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = std::fs::symlink_metadata(path).map_err(|error| UpdateError::io(path, error))?;
    if !metadata.file_type().is_file()
        || ArtifactIdentity::from_metadata(&metadata) != validated.identity
        || metadata.permissions().mode() & 0o7777 != validated.intended_mode()
    {
        return Err(UpdateError::ArtifactIdentityChanged {
            path: path.to_path_buf(),
        });
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

#[cfg(test)]
mod tests {
    use super::{
        open_backup_copy_destination, remove_and_sync, verify_or_cleanup_created_backup,
        write_backup_copy, write_backup_copy_with_cleanup,
    };

    struct FailingReader {
        yielded: bool,
    }

    impl std::io::Read for FailingReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if self.yielded {
                return Err(std::io::Error::other("injected copy failure"));
            }
            self.yielded = true;
            buffer[..4].copy_from_slice(b"part");
            Ok(4)
        }
    }

    #[test]
    fn backup_copy_error_removes_owned_partial_destination() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let backup = temp.path().join("partial-backup");
        let mut source = FailingReader { yielded: false };

        let error = write_backup_copy(&mut source, &backup, std::fs::Permissions::from_mode(0o700))
            .unwrap_err();

        assert!(matches!(error, crate::UpdateError::Io { .. }));
        assert!(!backup.exists());
    }

    #[test]
    fn backup_copy_cleanup_failure_preserves_both_errors() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let backup = temp.path().join("partial-backup");
        let mut source = FailingReader { yielded: false };

        let error = write_backup_copy_with_cleanup(
            &mut source,
            &backup,
            std::fs::Permissions::from_mode(0o700),
            |path| {
                assert_eq!(path, backup);
                assert_eq!(std::fs::read(path).unwrap(), b"part");
                Err(crate::UpdateError::io(
                    path,
                    std::io::Error::other("injected durable cleanup failure"),
                ))
            },
        )
        .unwrap_err();

        let crate::UpdateError::TransactionCleanupFailed { operation, cleanup } = error else {
            panic!("expected combined copy and cleanup error");
        };
        assert!(matches!(*operation, crate::UpdateError::Io { .. }));
        assert!(matches!(*cleanup, crate::UpdateError::Io { .. }));
        std::fs::remove_file(backup).unwrap();
    }

    #[test]
    fn hard_link_outer_sync_failure_is_durably_cleaned() {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("executable");
        let backup = temp.path().join("hard-link-backup");
        std::fs::write(&executable, b"confirmed executable").unwrap();
        std::fs::hard_link(&executable, &backup).unwrap();

        let error = verify_or_cleanup_created_backup(
            &backup,
            |path| {
                Err(crate::UpdateError::io(
                    path,
                    std::io::Error::other("injected outer sync failure"),
                ))
            },
            remove_and_sync,
        )
        .unwrap_err();

        assert!(matches!(error, crate::UpdateError::Io { .. }));
        assert_eq!(std::fs::read(executable).unwrap(), b"confirmed executable");
        assert!(!backup.exists());
    }

    #[test]
    fn hard_link_outer_cleanup_failure_preserves_both_errors() {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("executable");
        let backup = temp.path().join("hard-link-backup");
        std::fs::write(&executable, b"confirmed executable").unwrap();
        std::fs::hard_link(&executable, &backup).unwrap();

        let error = verify_or_cleanup_created_backup(
            &backup,
            |path| {
                Err(crate::UpdateError::io(
                    path,
                    std::io::Error::other("injected outer sync failure"),
                ))
            },
            |path| {
                assert!(path.exists());
                Err(crate::UpdateError::io(
                    path,
                    std::io::Error::other("injected durable cleanup failure"),
                ))
            },
        )
        .unwrap_err();

        let crate::UpdateError::TransactionCleanupFailed { operation, cleanup } = error else {
            panic!("expected combined outer sync and cleanup error");
        };
        assert!(matches!(*operation, crate::UpdateError::Io { .. }));
        assert!(matches!(*cleanup, crate::UpdateError::Io { .. }));
        assert_eq!(std::fs::read(executable).unwrap(), b"confirmed executable");
        assert_eq!(std::fs::read(&backup).unwrap(), b"confirmed executable");
        std::fs::remove_file(backup).unwrap();
    }

    #[test]
    fn backup_copy_destination_starts_with_source_mode_under_permissive_umask() {
        use std::os::unix::fs::PermissionsExt;

        const CHILD_ENV: &str = "SOMA_SELF_UPDATE_COPY_MODE_CHILD";
        if std::env::var_os(CHILD_ENV).is_some() {
            let _previous = nix::sys::stat::umask(nix::sys::stat::Mode::empty());
            let temp = tempfile::tempdir().unwrap();
            for mode in [0o700, 0o750] {
                let backup = temp.path().join(format!("backup-{mode:o}"));
                let file = open_backup_copy_destination(&backup, mode).unwrap();
                assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, mode);
            }
            return;
        }

        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "transaction::transaction_io::tests::backup_copy_destination_starts_with_source_mode_under_permissive_umask",
                "--nocapture",
            ])
            .env(CHILD_ENV, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "copy-mode child failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
