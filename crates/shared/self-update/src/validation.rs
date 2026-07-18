use std::process::Stdio;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use crate::{Result, StagedArtifact, UpdateError, Updater};

const OUTPUT_LIMIT: usize = 16 * 1024;

/// An artifact that executed successfully and reported its exact target version.
#[derive(Debug)]
pub struct ValidatedArtifact {
    pub(crate) staged: StagedArtifact,
}

impl ValidatedArtifact {
    pub fn path(&self) -> &std::path::Path {
        self.staged.path()
    }
    pub fn target_version(&self) -> &str {
        self.staged.target_version()
    }
    pub fn sha256(&self) -> &str {
        self.staged.sha256()
    }
}

impl Updater {
    /// Executes `--version` and consumes the staged artifact on success.
    pub async fn validate(&self, staged: StagedArtifact) -> Result<ValidatedArtifact> {
        let path = staged.path().to_path_buf();
        let mut child = spawn_validator(&path).await?;
        let stdout = child.stdout.take().expect("piped stdout is configured");
        let stderr = child.stderr.take().expect("piped stderr is configured");
        let stdout_task = tokio::spawn(read_bounded(stdout));
        let stderr_task = tokio::spawn(read_bounded(stderr));
        let timeout = self.policy().validation_timeout();
        let status = match tokio::time::timeout(timeout, child.wait()).await {
            Ok(result) => result.map_err(|error| UpdateError::io(&path, error))?,
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                stdout_task.abort();
                stderr_task.abort();
                return Err(UpdateError::ValidationTimedOut { timeout });
            }
        };
        let stdout = stdout_task
            .await
            .map_err(|error| UpdateError::io(&path, std::io::Error::other(error)))?
            .map_err(|error| UpdateError::io(&path, error))?;
        let stderr = stderr_task
            .await
            .map_err(|error| UpdateError::io(&path, std::io::Error::other(error)))?
            .map_err(|error| UpdateError::io(&path, error))?;
        if stdout.overflowed {
            return Err(UpdateError::ValidationOutputTooLarge {
                stream: "stdout",
                limit: OUTPUT_LIMIT,
            });
        }
        if stderr.overflowed {
            return Err(UpdateError::ValidationOutputTooLarge {
                stream: "stderr",
                limit: OUTPUT_LIMIT,
            });
        }
        let stderr_text = String::from_utf8_lossy(&stderr.bytes).into_owned();
        if !status.success() {
            return Err(UpdateError::ValidationFailed {
                code: status.code(),
                stderr: stderr_text,
            });
        }
        let output =
            String::from_utf8(stdout.bytes).map_err(|_| UpdateError::InvalidVersionOutput)?;
        let expected = staged.target_version();
        let matches = output.split_ascii_whitespace().any(|token| {
            token.trim_matches(|character: char| character.is_ascii_punctuation()) == expected
        });
        if !matches {
            return Err(UpdateError::VersionMismatch {
                expected: expected.to_owned(),
                output: output.trim().to_owned(),
            });
        }
        Ok(ValidatedArtifact { staged })
    }
}

async fn spawn_validator(path: &std::path::Path) -> Result<tokio::process::Child> {
    // Tokio's asynchronous file close can briefly race exec on Linux and
    // surface ETXTBSY even after the staged writer has been flushed and
    // converted back to a closed std file. Retry only that transient kernel
    // condition; every other spawn error remains immediate and typed.
    for _ in 0..10 {
        let result = Command::new(path)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn();
        match result {
            Ok(child) => return Ok(child),
            Err(error) if error.raw_os_error() == Some(26) => {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            Err(error) => return Err(UpdateError::io(path, error)),
        }
    }
    Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| UpdateError::io(path, error))
}

struct BoundedOutput {
    bytes: Vec<u8>,
    overflowed: bool,
}

async fn read_bounded(mut reader: impl AsyncRead + Unpin) -> std::io::Result<BoundedOutput> {
    let mut bytes = Vec::with_capacity(OUTPUT_LIMIT.min(4096));
    let mut buffer = [0_u8; 4096];
    let mut overflowed = false;
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let remaining = OUTPUT_LIMIT.saturating_sub(bytes.len());
        bytes.extend_from_slice(&buffer[..read.min(remaining)]);
        overflowed |= read > remaining;
    }
    Ok(BoundedOutput { bytes, overflowed })
}
