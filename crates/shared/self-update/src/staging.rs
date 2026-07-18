use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use crate::{Result, UpdateDirective, UpdateError, Updater};

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A fully downloaded artifact whose digest matches its directive.
#[derive(Debug)]
pub struct StagedArtifact {
    pub(crate) path: PathBuf,
    pub(crate) target_version: String,
    pub(crate) sha256: String,
    bytes_written: u64,
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
}

impl Drop for StagedArtifact {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
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
}

impl Drop for PartialArtifact {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
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
        let directory = self
            .layout()
            .executable()
            .parent()
            .ok_or(UpdateError::InvalidPolicy(
                "executable must have a parent directory",
            ))?;
        let name = self
            .layout()
            .executable()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("executable");
        let path = directory.join(format!(
            ".{name}.update-{}-{}.part",
            std::process::id(),
            STAGING_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let mut cleanup = PartialArtifact {
            path: path.clone(),
            armed: true,
        };
        let mut file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .await
            .map_err(|error| UpdateError::io(&path, error))?;
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
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .await
                .map_err(|error| UpdateError::io(&path, error))?;
            file.sync_all()
                .await
                .map_err(|error| UpdateError::io(&path, error))?;
        }
        // Wait for Tokio's blocking file operations to relinquish ownership,
        // then synchronously close the writable descriptor before callers
        // execute the path. Linux rejects an executable still open for writing.
        drop(file.into_std().await);
        let actual = encode_hex(&hasher.finalize());
        if actual != directive.sha256() {
            return Err(UpdateError::DigestMismatch {
                expected: directive.sha256().to_owned(),
                actual,
            });
        }
        cleanup.disarm();
        Ok(StagedArtifact {
            path,
            target_version: directive.version().to_owned(),
            sha256: actual,
            bytes_written: total,
        })
    }
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
