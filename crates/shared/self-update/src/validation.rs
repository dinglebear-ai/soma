use std::process::Stdio;

#[cfg(windows)]
use process_wrap::tokio::JobObject;
#[cfg(unix)]
use process_wrap::tokio::ProcessGroup;
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::{Result, StagedArtifact, UpdateError, Updater};

const OUTPUT_LIMIT: usize = 16 * 1024;

/// An artifact that executed successfully and reported its exact target version.
#[derive(Debug)]
pub struct ValidatedArtifact {
    pub(crate) staged: StagedArtifact,
    #[cfg(unix)]
    pub(crate) identity: ArtifactIdentity,
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ArtifactIdentity {
    device: u64,
    inode: u64,
}

#[cfg(unix)]
impl ArtifactIdentity {
    pub(crate) fn from_metadata(metadata: &std::fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
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

    #[cfg(unix)]
    pub(crate) fn intended_mode(&self) -> u32 {
        self.staged.intended_mode
    }

    #[cfg(unix)]
    pub(crate) fn revalidate_source_executable(&self, path: &std::path::Path) -> Result<()> {
        self.staged.revalidate_source_executable(path)
    }
}

impl Updater {
    /// Executes `--version` and consumes the staged artifact on success.
    pub async fn validate(&self, staged: StagedArtifact) -> Result<ValidatedArtifact> {
        let path = staged.path().to_path_buf();
        #[cfg(unix)]
        let identity = validated_path_identity(&path)?;
        let timeout = self.policy().validation_timeout();
        let deadline = tokio::time::Instant::now() + timeout;
        let child = match tokio::time::timeout_at(deadline, spawn_validator(&path)).await {
            Ok(result) => result?,
            Err(_) => return Err(UpdateError::ValidationTimedOut { timeout }),
        };
        let mut child = ValidationProcessGuard::new(child);
        let stdout = child
            .child_mut()
            .stdout()
            .take()
            .expect("piped stdout is configured");
        let stderr = child
            .child_mut()
            .stderr()
            .take()
            .expect("piped stderr is configured");
        let completed = tokio::time::timeout_at(deadline, async {
            // Descendants may inherit the validator's output handles. Tear down
            // the whole group as soon as the leader exits so the readers can
            // observe EOF without turning a successful validation into a timeout.
            let status = async {
                let status = child.leader_mut().wait().await;
                let terminated = child.terminate_and_drain(&path).await;
                match (status, terminated) {
                    (Ok(status), Ok(())) => Ok(status),
                    (Err(error), _) => Err(UpdateError::io(&path, error)),
                    (Ok(_), Err(error)) => Err(error),
                }
            };
            let (status, stdout, stderr) =
                tokio::join!(status, read_bounded(stdout), read_bounded(stderr));
            (status, stdout, stderr)
        })
        .await;
        let (status, stdout, stderr) = match completed {
            Ok((status, stdout, stderr)) => (
                status?,
                stdout.map_err(|error| UpdateError::io(&path, error))?,
                stderr.map_err(|error| UpdateError::io(&path, error))?,
            ),
            Err(_) => {
                let _ = Box::into_pin(child.child_mut().kill()).await;
                child.disarm();
                return Err(UpdateError::ValidationTimedOut { timeout });
            }
        };
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
        #[cfg(unix)]
        if validated_path_identity(&path)? != identity {
            return Err(UpdateError::ArtifactIdentityChanged { path });
        }
        Ok(ValidatedArtifact {
            staged,
            #[cfg(unix)]
            identity,
        })
    }
}

struct ValidationProcessGuard {
    child: Box<dyn ChildWrapper>,
    armed: bool,
}

impl ValidationProcessGuard {
    fn new(child: Box<dyn ChildWrapper>) -> Self {
        Self { child, armed: true }
    }

    fn child_mut(&mut self) -> &mut dyn ChildWrapper {
        self.child.as_mut()
    }

    fn leader_mut(&mut self) -> &mut dyn ChildWrapper {
        self.child.inner_mut()
    }

    async fn terminate_and_drain(&mut self, path: &std::path::Path) -> Result<()> {
        match self.child.start_kill() {
            Ok(()) => {}
            #[cfg(unix)]
            Err(error) if error.raw_os_error() == Some(nix::libc::ESRCH) => {}
            Err(error) => return Err(UpdateError::io(path, error)),
        }
        self.child
            .wait()
            .await
            .map_err(|error| UpdateError::io(path, error))?;
        self.disarm();
        Ok(())
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ValidationProcessGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.child.start_kill();
        }
    }
}

#[cfg(unix)]
fn validated_path_identity(path: &std::path::Path) -> Result<ArtifactIdentity> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| UpdateError::io(path, error))?;
    if !metadata.file_type().is_file() {
        return Err(UpdateError::InvalidStagedArtifact {
            path: path.to_path_buf(),
        });
    }
    Ok(ArtifactIdentity::from_metadata(&metadata))
}

async fn spawn_validator(path: &std::path::Path) -> Result<Box<dyn ChildWrapper>> {
    // Tokio's asynchronous file close can briefly race exec on Linux and
    // surface ETXTBSY even after the staged writer has been flushed and
    // converted back to a closed std file. Retry only that transient kernel
    // condition; every other spawn error remains immediate and typed.
    for _ in 0..10 {
        let result = validator_command(path).spawn();
        match result {
            Ok(child) => return Ok(child),
            Err(error) if error.raw_os_error() == Some(26) => {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            Err(error) => return Err(UpdateError::io(path, error)),
        }
    }
    validator_command(path)
        .spawn()
        .map_err(|error| UpdateError::io(path, error))
}

fn validator_command(path: &std::path::Path) -> CommandWrap {
    let mut command = CommandWrap::with_new(path, |command| {
        command
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    });
    command.wrap(KillOnDrop);
    #[cfg(unix)]
    command.wrap(ProcessGroup::leader());
    #[cfg(windows)]
    command.wrap(JobObject);
    command
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
