use std::{io, process::Output, process::Stdio, time::Duration};

use soma_contracts::providers::EnvRequirement;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
    task::JoinError,
    time::timeout,
};

use crate::{provider_errors::ProviderError, provider_registry::ProviderCall};

pub(crate) struct BoundedOutput {
    pub output: Output,
    pub stdout_exceeded: bool,
    pub stderr_exceeded: bool,
}

#[derive(Debug)]
pub(crate) enum SidecarError {
    Io(io::Error),
    Join(JoinError),
    Timeout,
}

impl std::fmt::Display for SidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Join(error) => write!(f, "{error}"),
            Self::Timeout => write!(f, "sidecar process timed out"),
        }
    }
}

impl std::error::Error for SidecarError {}

pub(crate) async fn run_bounded_sidecar(
    command: &str,
    args: &[&str],
    env: Vec<(String, String)>,
    input: &[u8],
    timeout_ms: u64,
    max_output_bytes: usize,
) -> Result<BoundedOutput, SidecarError> {
    let mut child = Command::new(command)
        .args(args)
        .kill_on_drop(true)
        .env_clear()
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(SidecarError::Io)?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("sidecar stdout pipe was not captured"))
        .map_err(SidecarError::Io)?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("sidecar stderr pipe was not captured"))
        .map_err(SidecarError::Io)?;
    let stdout_task = tokio::spawn(read_bounded(stdout, max_output_bytes));
    let stderr_task = tokio::spawn(read_bounded(stderr, max_output_bytes));

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input).await.map_err(SidecarError::Io)?;
    }

    let status = match timeout(Duration::from_millis(timeout_ms), child.wait()).await {
        Ok(status) => status.map_err(SidecarError::Io)?,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            stdout_task.abort();
            stderr_task.abort();
            return Err(SidecarError::Timeout);
        }
    };

    let (stdout, stdout_exceeded) = stdout_task
        .await
        .map_err(SidecarError::Join)?
        .map_err(SidecarError::Io)?;
    let (stderr, stderr_exceeded) = stderr_task
        .await
        .map_err(SidecarError::Join)?
        .map_err(SidecarError::Io)?;

    Ok(BoundedOutput {
        output: Output {
            status,
            stdout,
            stderr,
        },
        stdout_exceeded,
        stderr_exceeded,
    })
}

pub(crate) fn output_exceeded_message(stream: &str, max_output_bytes: usize) -> String {
    format!("sidecar {stream} output exceeds {max_output_bytes} bytes")
}

pub(crate) fn collect_provider_env(
    provider_requirements: &[EnvRequirement],
    tool_requirements: &[EnvRequirement],
    call: &ProviderCall,
) -> Result<Vec<(String, String)>, ProviderError> {
    let mut env = Vec::new();
    for requirement in provider_requirements.iter().chain(tool_requirements) {
        let name = requirement.runtime_name("SOMA");
        let value = std::env::var(&name)
            .ok()
            .or_else(|| {
                requirement
                    .allow_unprefixed
                    .then(|| std::env::var(&requirement.name).ok())
                    .flatten()
            })
            .or_else(|| {
                requirement
                    .default
                    .as_ref()
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned)
            });
        match value {
            Some(value) => env.push((name, value)),
            None if requirement.required => {
                return Err(ProviderError::validation(
                    &call.provider,
                    &call.action,
                    "missing_provider_env",
                    format!("missing required provider env `{name}`"),
                ));
            }
            None => {}
        }
    }
    Ok(env)
}

async fn read_bounded<R>(mut reader: R, max_output_bytes: usize) -> io::Result<(Vec<u8>, bool)>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    let mut exceeded = false;
    let mut chunk = [0u8; 8192];
    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            return Ok((bytes, exceeded));
        }
        let remaining = max_output_bytes.saturating_sub(bytes.len());
        if remaining >= read && !exceeded {
            bytes.extend_from_slice(&chunk[..read]);
        } else {
            exceeded = true;
            if remaining > 0 {
                bytes.extend_from_slice(&chunk[..remaining]);
            }
        }
    }
}
