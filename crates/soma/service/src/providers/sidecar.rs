use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
    process::Output,
    process::Stdio,
    time::Duration,
};

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
    let resolved_command = resolve_sidecar_command(command);
    let mut command = Command::new(resolved_command);
    command
        .args(args)
        .kill_on_drop(true)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    apply_sidecar_base_env(&mut command);
    command.envs(env);

    let mut child = command.spawn().map_err(SidecarError::Io)?;

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

fn apply_sidecar_base_env(command: &mut Command) {
    for (key, value) in sidecar_base_env() {
        command.env(key, value);
    }
}

#[cfg(windows)]
pub(crate) fn sidecar_base_env() -> Vec<(OsString, OsString)> {
    let mut env = Vec::new();
    for key in ["SystemRoot", "WINDIR", "COMSPEC", "PATHEXT", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(key) {
            env.push((OsString::from(key), value));
        }
    }
    env
}

#[cfg(not(windows))]
pub(crate) fn sidecar_base_env() -> Vec<(OsString, OsString)> {
    let mut env = Vec::new();
    for key in ["HOME", "TMPDIR", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(key) {
            env.push((OsString::from(key), value));
        }
    }
    env
}

pub(crate) fn resolve_sidecar_command(command: &str) -> PathBuf {
    resolve_sidecar_command_with_env(
        command,
        std::env::var_os("PATH"),
        std::env::var_os("PATHEXT"),
    )
}

fn resolve_sidecar_command_with_env(
    command: &str,
    path_env: Option<OsString>,
    pathext_env: Option<OsString>,
) -> PathBuf {
    let command_path = Path::new(command);
    if command_path.components().count() > 1 || command_path.is_absolute() {
        return command_path.to_path_buf();
    }

    let Some(path_env) = path_env else {
        return command_path.to_path_buf();
    };
    for dir in std::env::split_paths(&path_env) {
        if command_path.extension().is_some() {
            let candidate = dir.join(command_path);
            if candidate.is_file() {
                return resolve_runtime_shim(command, candidate);
            }
            continue;
        }
        let direct_candidate = dir.join(command_path);
        if direct_candidate.is_file() {
            return resolve_runtime_shim(command, direct_candidate);
        }
        #[cfg(windows)]
        for extension in windows_path_extensions(pathext_env.as_ref()) {
            let candidate = dir.join(format!("{command}{extension}"));
            if candidate.is_file() {
                return resolve_runtime_shim(command, candidate);
            }
        }
    }
    #[cfg(not(windows))]
    let _ = pathext_env;
    command_path.to_path_buf()
}

fn resolve_runtime_shim(command: &str, candidate: PathBuf) -> PathBuf {
    resolve_mise_shim(command, &candidate).unwrap_or(candidate)
}

fn resolve_mise_shim(command: &str, candidate: &Path) -> Option<PathBuf> {
    let canonical = candidate.canonicalize().ok()?;
    if canonical.file_stem()?.to_string_lossy() != "mise" {
        return None;
    }
    let output = std::process::Command::new(&canonical)
        .args(["which", command])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let resolved = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    resolved.is_file().then_some(resolved)
}

#[cfg(windows)]
fn windows_path_extensions(pathext_env: Option<&OsString>) -> Vec<String> {
    pathext_env
        .and_then(|value| value.to_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_owned())
        .split(';')
        .filter(|extension| !extension.is_empty())
        .map(|extension| {
            if extension.starts_with('.') {
                extension.to_owned()
            } else {
                format!(".{extension}")
            }
        })
        .collect()
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

#[cfg(test)]
#[path = "sidecar_tests.rs"]
mod tests;
