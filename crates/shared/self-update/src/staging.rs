use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use crate::{Result, UpdateDirective, UpdateError, Updater, reject_executable_leaf_symlink};
#[cfg(unix)]
use crate::transaction::path_validation::validate_distinct_paths;

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);
#[cfg(unix)]
const VALIDATION_MODE: u32 = 0o700;

/// A fully downloaded artifact whose digest matches its directive.
#[derive(Debug)]
pub struct StagedArtifact {
    pub(crate) path: PathBuf,
    pub(crate) target_version: String,
    pub(crate) sha256: String,
    bytes_written: u64,
    cleanup_on_drop: bool,
    #[cfg(unix)]
    pub(crate) intended_mode: u32,
    #[cfg(unix)]
    source_identity: ExecutableIdentity,
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExecutableIdentity {
    Present {
        device: u64,
        inode: u64,
        mode: u32,
    },
    Absent,
}

impl StagedArtifact {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn target_version(&self) -> &str {
        &self.target_version
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    #[cfg(unix)]
    pub(crate) fn revalidate_source_executable(&self, path: &Path) -> Result<()> {
        let (_, current) = executable_mode_and_identity(path)?;
        if current != self.source_identity {
            return Err(UpdateError::ExecutableIdentityChanged {
                path: path.to_path_buf(),
            });
        }
        Ok(())
    }

    /// Explicitly removes the staged file and reports cleanup failures.
    ///
    /// Dropping an artifact remains a best-effort fallback because `Drop`
    /// cannot return an error.
    pub fn cleanup(mut self) -> Result<()> {
        std::fs::remove_file(&self.path).map_err(|error| UpdateError::io(&self.path, error))?;
        self.cleanup_on_drop = false;
        Ok(())
    }
}

impl Drop for StagedArtifact {
    fn drop(&mut self) {
        if self.cleanup_on_drop {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

struct PartialArtifact {
    path: PathBuf,
    armed: bool,
}

impl PartialArtifact {
    fn disarm(&mut self) {
        self.armed = false;
    }

    fn report_error(mut self, operation: UpdateError) -> UpdateError {
        self.armed = false;
        match std::fs::remove_file(&self.path) {
            Ok(()) => operation,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => operation,
            Err(cleanup) => UpdateError::ArtifactCleanupFailed {
                path: self.path.clone(),
                operation: Box::new(operation),
                cleanup,
            },
        }
    }
}

impl Drop for PartialArtifact {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

async fn create_partial(path: &Path) -> Result<(tokio::fs::File, PartialArtifact)> {
    let mut open_options = tokio::fs::OpenOptions::new();
    open_options.create_new(true).write(true);
    #[cfg(unix)]
    {
        open_options.mode(0o600);
    }
    let file = open_options
        .open(path)
        .await
        .map_err(|error| UpdateError::io(path, error))?;
    let cleanup = PartialArtifact {
        path: path.to_path_buf(),
        armed: true,
    };
    Ok((file, cleanup))
}

impl Updater {
    pub async fn stage<R>(
        &self,
        mut reader: R,
        directive: &UpdateDirective,
    ) -> Result<StagedArtifact>
    where
        R: AsyncRead + Unpin,
    {
        self.ensure_layout_bound()?;
        reject_executable_leaf_symlink(self.layout().executable())?;
        #[cfg(unix)]
        let layout = self.validated_layout()?;
        #[cfg(unix)]
        let resolved_executable = &layout.executable;
        #[cfg(not(unix))]
        let resolved_executable = self.layout().executable();
        let directory = resolved_executable
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let name = resolved_executable
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("executable");
        let path = directory.join(format!(
            ".{name}.update-{}-{}.part",
            std::process::id(),
            STAGING_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        #[cfg(unix)]
        let (intended_mode, source_identity) = executable_mode_and_identity(resolved_executable)?;
        #[cfg(unix)]
        {
            let mut namespace = layout.protected.clone();
            namespace.push(path.clone());
            validate_distinct_paths(&namespace)?;
        }
        let (mut file, mut cleanup) = create_partial(&path).await?;
        let result: Result<(u64, String)> = async {
            let mut buffer = [0_u8; 64 * 1024];
            let mut total = 0_u64;
            let mut hasher = Sha256::new();
            loop {
                let read = reader
                    .read(&mut buffer)
                    .await
                    .map_err(|error| UpdateError::io(&path, error))?;
                if read == 0 {
                    break;
                }
                let next = total.saturating_add(read as u64);
                if next > self.policy().max_artifact_bytes() {
                    return Err(UpdateError::ArtifactTooLarge {
                        limit: self.policy().max_artifact_bytes(),
                        actual: next,
                    });
                }
                file.write_all(&buffer[..read])
                    .await
                    .map_err(|error| UpdateError::io(&path, error))?;
                hasher.update(&buffer[..read]);
                total = next;
            }
            file.flush()
                .await
                .map_err(|error| UpdateError::io(&path, error))?;
            file.sync_all()
                .await
                .map_err(|error| UpdateError::io(&path, error))?;
            let actual = encode_hex(&hasher.finalize());
            if actual != directive.sha256() {
                return Err(UpdateError::DigestMismatch {
                    expected: directive.sha256().to_owned(),
                    actual,
                });
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(std::fs::Permissions::from_mode(VALIDATION_MODE))
                    .await
                    .map_err(|error| UpdateError::io(&path, error))?;
                file.sync_all()
                    .await
                    .map_err(|error| UpdateError::io(&path, error))?;
            }
            Ok((total, actual))
        }
        .await;
        // Close the writable descriptor before explicit error cleanup or
        // before callers execute the successful artifact.
        drop(file.into_std().await);
        let (total, actual) = match result {
            Ok(result) => result,
            Err(operation) => return Err(cleanup.report_error(operation)),
        };
        cleanup.disarm();
        Ok(StagedArtifact {
            path,
            target_version: directive.version().to_owned(),
            sha256: actual,
            bytes_written: total,
            cleanup_on_drop: true,
            #[cfg(unix)]
            intended_mode,
            #[cfg(unix)]
            source_identity,
        })
    }
}

#[cfg(unix)]
fn executable_mode_and_identity(path: &Path) -> Result<(u32, ExecutableIdentity)> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

    let file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((0o700, ExecutableIdentity::Absent));
        }
        Err(error) => return Err(UpdateError::io(path, error)),
    };
    let metadata = file
        .metadata()
        .map_err(|error| UpdateError::io(path, error))?;
    if !metadata.file_type().is_file() {
        return Err(UpdateError::InvalidPolicy(
            "executable path must be a regular file",
        ));
    }
    let mode = metadata.permissions().mode() & 0o7777;
    Ok((
        mode,
        ExecutableIdentity::Present {
            device: metadata.dev(),
            inode: metadata.ino(),
            mode,
        },
    ))
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_partial_collision_preserves_preexisting_sentinel() {
        let temp = tempfile::tempdir().unwrap();
        let explicit_path = temp.path().join("explicit-collision.part");
        let sentinel = b"not owned by the updater";
        std::fs::write(&explicit_path, sentinel).unwrap();

        assert!(create_partial(&explicit_path).await.is_err());
        assert_eq!(std::fs::read(&explicit_path).unwrap(), sentinel);
    }
}
