//! The generic Python (/ LangChain / LlamaIndex) provider kind: introspects
//! and executes a drop-in `.py` provider through a bounded Python sidecar
//! running `python_bridge::PYTHON_BRIDGE`. Ported from
//! `soma-service::providers::python`.
//!
//! `load_python_catalog` here applies only *generic* manifest validation
//! (`soma_provider_core::validate_provider_manifest_value`) — Soma's own CLI
//! reserved-command / env-prefix policy layer is applied downstream by the
//! host's provider registry when it builds every provider's catalog
//! (regardless of kind), so dropping the redundant Soma-specific pre-check
//! here does not weaken overall enforcement — see the PR10 deviation notes.

use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command as StdCommand, Stdio as StdStdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use serde_json::{json, Value};
use soma_provider_core::{
    validate_provider_manifest_value, Provider, ProviderCall, ProviderCatalog, ProviderError,
    ProviderOutput, ProviderTool,
};
use tokio::time::Instant as TokioInstant;

use crate::{
    error::{redact_public, SidecarError},
    python_bridge::PYTHON_BRIDGE,
    sidecar::{
        collect_provider_env, output_exceeded_message, resolve_sidecar_command,
        run_bounded_sidecar, sidecar_base_env,
    },
};

const DEFAULT_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_MAX_INPUT_BYTES: usize = 64 * 1024;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Clone)]
pub struct PythonProvider {
    path: PathBuf,
    catalog: ProviderCatalog,
    env_prefix: String,
}

impl PythonProvider {
    pub fn new(path: PathBuf, catalog: ProviderCatalog, env_prefix: impl Into<String>) -> Self {
        Self {
            path,
            catalog,
            env_prefix: env_prefix.into(),
        }
    }

    pub fn arc(
        path: PathBuf,
        catalog: ProviderCatalog,
        env_prefix: impl Into<String>,
    ) -> Arc<Self> {
        Arc::new(Self::new(path, catalog, env_prefix))
    }
}

#[async_trait]
impl Provider for PythonProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        let tool = self.tool(&call)?;
        let runtime = PythonRuntime::from_tool(&self.catalog, tool, &call, &self.env_prefix)?;
        let source = self.path.display().to_string();
        let input = python_execution_payload(&self.path, &call, &runtime.env).map_err(|error| {
            ProviderError::execution(&self.catalog.provider.name, "", error)
                .with_provider_kind(self.catalog.provider.kind.as_str())
                .with_source(source.clone())
                .with_phase("input-serialization")
        })?;

        if input.len() > runtime.max_input_bytes {
            return Err(ProviderError::validation(
                &self.catalog.provider.name,
                &call.action,
                "python_input_too_large",
                format!(
                    "Python provider input exceeds {} bytes",
                    runtime.max_input_bytes
                ),
            )
            .with_provider_kind(self.catalog.provider.kind.as_str())
            .with_source(source)
            .with_phase("input-validation"));
        }

        let started = TokioInstant::now();
        let sidecar = match run_bounded_sidecar(
            &runtime.command,
            &["-c", PYTHON_BRIDGE],
            runtime.env,
            &input,
            runtime.timeout_ms,
            runtime.max_output_bytes,
        )
        .await
        {
            Ok(sidecar) => sidecar,
            Err(SidecarError::Timeout) => {
                return Err(ProviderError::new(
                    "python_provider_timeout",
                    &self.catalog.provider.name,
                    Some(call.action.clone()),
                    format!("Python provider exceeded {}ms timeout", runtime.timeout_ms),
                    "Increase tool.limits.timeout_ms or fix the Python provider handler.",
                )
                .with_provider_kind(self.catalog.provider.kind.as_str())
                .with_source(source)
                .with_phase("execution"));
            }
            Err(error) => {
                return Err(ProviderError::execution(
                    &self.catalog.provider.name,
                    call.action.clone(),
                    error,
                )
                .with_provider_kind(self.catalog.provider.kind.as_str())
                .with_source(source)
                .with_phase("execution"));
            }
        };
        let output = sidecar.output;

        tracing::debug!(
            provider = %self.catalog.provider.name,
            action = %call.action,
            elapsed_ms = started.elapsed().as_millis(),
            "Python provider sidecar completed"
        );

        if sidecar.stdout_exceeded || sidecar.stderr_exceeded {
            let stream = if sidecar.stdout_exceeded {
                "stdout"
            } else {
                "stderr"
            };
            return Err(ProviderError::validation(
                &self.catalog.provider.name,
                &call.action,
                "python_output_too_large",
                output_exceeded_message(stream, runtime.max_output_bytes),
            )
            .with_provider_kind(self.catalog.provider.kind.as_str())
            .with_source(source)
            .with_phase("output-validation"));
        }
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let code = if stderr.contains("python_provider_unserializable_output") {
                "python_provider_unserializable_output"
            } else {
                "python_provider_failed"
            };
            return Err(ProviderError::new(
                code,
                &self.catalog.provider.name,
                Some(call.action),
                format!("Python provider failed: {}", redact_public(&stderr)),
                "Fix the Python provider handler and retry.",
            )
            .with_provider_kind(self.catalog.provider.kind.as_str())
            .with_source(source)
            .with_phase("execution"));
        }

        let value = serde_json::from_slice(&output.stdout).map_err(|error| {
            ProviderError::validation(
                &self.catalog.provider.name,
                &call.action,
                "python_invalid_json_output",
                error.to_string(),
            )
            .with_provider_kind(self.catalog.provider.kind.as_str())
            .with_source(source)
            .with_phase("output-validation")
        })?;
        Ok(ProviderOutput::json(value))
    }
}

fn python_execution_payload(
    path: &Path,
    call: &ProviderCall,
    env: &[(String, String)],
) -> Result<Vec<u8>, serde_json::Error> {
    let mut payload = serde_json::to_value(crate::sidecar::ExecutionEnvelope::new(call))?;
    if let Some(object) = payload.as_object_mut() {
        let env_keys: Vec<&str> = env.iter().map(|(key, _)| key.as_str()).collect();
        object.insert("mode".to_owned(), json!("call"));
        object.insert("path".to_owned(), json!(path.to_path_buf()));
        object.insert("env_keys".to_owned(), json!(env_keys));
    }
    serde_json::to_vec(&payload)
}

impl PythonProvider {
    fn tool(&self, call: &ProviderCall) -> Result<&ProviderTool, ProviderError> {
        self.catalog
            .tools
            .iter()
            .find(|tool| tool.name == call.action)
            .ok_or_else(|| {
                ProviderError::validation(
                    &self.catalog.provider.name,
                    &call.action,
                    "unknown_python_action",
                    format!("Python provider has no action `{}`", call.action),
                )
            })
    }
}

/// Introspects a `.py` provider file by importing it (in "catalog" mode) in
/// a bounded sidecar and validating the resulting manifest against the
/// generic provider-core contract. Callers that layer additional product
/// policy on top of every provider's catalog (e.g. Soma's reserved
/// CLI-command / env-prefix checks) apply it to the returned catalog
/// themselves — see the module docs above.
pub fn load_python_catalog(path: &Path, env_prefix: &str) -> Result<ProviderCatalog, String> {
    let runtime = PythonRuntime::for_catalog(env_prefix);
    let input = serde_json::to_vec(&json!({
        "mode": "catalog",
        "path": path,
    }))
    .map_err(|error| error.to_string())?;
    let output = run_catalog_sidecar(&runtime, &input)?;
    let value: Value = serde_json::from_slice(&output).map_err(|error| error.to_string())?;
    validate_provider_manifest_value(&value).map_err(|error| error.to_string())
}

struct PythonRuntime {
    command: String,
    env: Vec<(String, String)>,
    timeout_ms: u64,
    max_input_bytes: usize,
    max_output_bytes: usize,
}

impl PythonRuntime {
    fn for_catalog(env_prefix: &str) -> Self {
        let prefix = env_prefix.trim_matches('_').to_ascii_uppercase();
        let timeout_var = format!("{prefix}_PYTHON_CATALOG_TIMEOUT_MS");
        let timeout_ms = match std::env::var(&timeout_var) {
            Ok(value) => value.parse().unwrap_or_else(|error| {
                tracing::warn!(
                    variable = %timeout_var,
                    value,
                    error = %error,
                    "invalid provider catalog timeout env var; falling back to the default"
                );
                DEFAULT_TIMEOUT_MS
            }),
            Err(_) => DEFAULT_TIMEOUT_MS,
        };
        Self {
            command: std::env::var(format!("{prefix}_PYTHON_COMMAND"))
                .unwrap_or_else(|_| default_python_command().to_owned()),
            env: Vec::new(),
            timeout_ms,
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        }
    }

    fn from_tool(
        catalog: &ProviderCatalog,
        tool: &ProviderTool,
        call: &ProviderCall,
        env_prefix: &str,
    ) -> Result<Self, ProviderError> {
        let provider_meta = catalog.meta.get("python");
        let tool_meta = tool.meta.get("python");
        let meta_field = |key: &str| {
            tool_meta
                .and_then(|value| value.get(key))
                .or_else(|| provider_meta.and_then(|value| value.get(key)))
        };
        let command = meta_field("command")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                std::env::var(format!(
                    "{}_PYTHON_COMMAND",
                    env_prefix.trim_matches('_').to_ascii_uppercase()
                ))
                .ok()
            })
            .unwrap_or_else(|| default_python_command().to_owned());
        let timeout_ms = tool
            .limits
            .as_ref()
            .and_then(|limits| limits.timeout_ms)
            .or_else(|| meta_field("timeout_ms").and_then(Value::as_u64))
            .unwrap_or(DEFAULT_TIMEOUT_MS);
        let max_input_bytes = tool
            .limits
            .as_ref()
            .and_then(|limits| limits.max_input_bytes)
            .unwrap_or(DEFAULT_MAX_INPUT_BYTES);
        let max_output_bytes = tool
            .limits
            .as_ref()
            .and_then(|limits| limits.max_response_bytes)
            .unwrap_or(DEFAULT_MAX_OUTPUT_BYTES);
        Ok(Self {
            command,
            env: collect_provider_env(
                &catalog.env,
                &tool.env,
                env_prefix,
                &call.provider,
                &call.action,
            )?,
            timeout_ms,
            max_input_bytes,
            max_output_bytes,
        })
    }
}

#[cfg(windows)]
fn default_python_command() -> &'static str {
    "python"
}

#[cfg(not(windows))]
fn default_python_command() -> &'static str {
    "python3"
}

fn run_catalog_sidecar(runtime: &PythonRuntime, input: &[u8]) -> Result<Vec<u8>, String> {
    let mut command = StdCommand::new(resolve_sidecar_command(&runtime.command));
    command
        .args(["-c", PYTHON_BRIDGE])
        .env_clear()
        .stdin(StdStdio::piped())
        .stdout(StdStdio::piped())
        .stderr(StdStdio::piped());
    for (key, value) in sidecar_base_env() {
        command.env(key, value);
    }
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Python provider catalog stdout pipe was not captured".to_owned())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Python provider catalog stderr pipe was not captured".to_owned())?;
    let max_output_bytes = runtime.max_output_bytes;
    let stdout_task = thread::spawn(move || read_bounded_sync(stdout, max_output_bytes));
    let stderr_task = thread::spawn(move || read_bounded_sync(stderr, max_output_bytes));

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input).map_err(|error| error.to_string())?;
    }
    let deadline = Instant::now() + Duration::from_millis(runtime.timeout_ms);
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            let (stdout, stdout_exceeded) = stdout_task
                .join()
                .map_err(|_| "Python provider catalog stdout reader panicked".to_owned())?
                .map_err(|error| error.to_string())?;
            let (stderr, stderr_exceeded) = stderr_task
                .join()
                .map_err(|_| "Python provider catalog stderr reader panicked".to_owned())?
                .map_err(|error| error.to_string())?;
            if stdout_exceeded || stderr_exceeded {
                let stream = if stdout_exceeded { "stdout" } else { "stderr" };
                return Err(format!(
                    "Python provider catalog {}",
                    output_exceeded_message(stream, runtime.max_output_bytes)
                ));
            }
            if !status.success() {
                return Err(format!(
                    "Python provider catalog failed: {}",
                    redact_public(&String::from_utf8_lossy(&stderr))
                ));
            }
            return Ok(stdout);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "Python provider catalog exceeded {}ms timeout",
                runtime.timeout_ms
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn read_bounded_sync<R: Read>(
    mut reader: R,
    max_output_bytes: usize,
) -> std::io::Result<(Vec<u8>, bool)> {
    let mut bytes = Vec::new();
    let mut exceeded = false;
    let mut chunk = [0u8; 8192];
    loop {
        let read = reader.read(&mut chunk)?;
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
#[path = "python_tests.rs"]
mod tests;
