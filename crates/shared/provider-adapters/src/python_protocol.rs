use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use soma_provider_core::{ProviderCall, ProviderSurface};
use thiserror::Error;

pub(crate) const PYTHON_WORKER_SCHEMA_VERSION: u32 = 1;
pub(crate) const ONE_SHOT_REQUEST_ID: u64 = 0;
pub(crate) const PYTHON_PROTOCOL_HEADROOM_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub(crate) enum PythonWorkerRequest {
    Catalog {
        schema_version: u32,
        request_id: u64,
        path: PathBuf,
    },
    Call {
        schema_version: u32,
        request_id: u64,
        path: PathBuf,
        #[serde(default)]
        env_keys: Vec<String>,
        provider: String,
        action: String,
        params: Value,
        surface: ProviderSurface,
        snapshot_id: String,
    },
}

impl PythonWorkerRequest {
    pub(crate) fn catalog(path: &Path) -> Self {
        Self::Catalog {
            schema_version: PYTHON_WORKER_SCHEMA_VERSION,
            request_id: ONE_SHOT_REQUEST_ID,
            path: path.to_path_buf(),
        }
    }

    pub(crate) fn call(path: &Path, call: &ProviderCall, env_keys: Vec<String>) -> Self {
        Self::Call {
            schema_version: PYTHON_WORKER_SCHEMA_VERSION,
            request_id: ONE_SHOT_REQUEST_ID,
            path: path.to_path_buf(),
            env_keys,
            provider: call.provider.clone(),
            action: call.action.clone(),
            params: call.params.clone(),
            surface: call.surface,
            snapshot_id: call.snapshot_id.clone(),
        }
    }

    fn schema_version(&self) -> u32 {
        match self {
            Self::Catalog { schema_version, .. } | Self::Call { schema_version, .. } => {
                *schema_version
            }
        }
    }

    fn request_id(&self) -> u64 {
        match self {
            Self::Catalog { request_id, .. } | Self::Call { request_id, .. } => *request_id,
        }
    }

    fn mode(&self) -> &'static str {
        match self {
            Self::Catalog { .. } => "catalog",
            Self::Call { .. } => "call",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub(crate) enum PythonWorkerResponse {
    Catalog {
        schema_version: u32,
        request_id: u64,
        catalog: Value,
    },
    Call {
        schema_version: u32,
        request_id: u64,
        output: Value,
    },
}

impl PythonWorkerResponse {
    fn schema_version(&self) -> u32 {
        match self {
            Self::Catalog { schema_version, .. } | Self::Call { schema_version, .. } => {
                *schema_version
            }
        }
    }

    fn request_id(&self) -> u64 {
        match self {
            Self::Catalog { request_id, .. } | Self::Call { request_id, .. } => *request_id,
        }
    }

    fn mode(&self) -> &'static str {
        match self {
            Self::Catalog { .. } => "catalog",
            Self::Call { .. } => "call",
        }
    }
}

/// Current major version of the persistent Python runner protocol.
pub const PYTHON_RUNNER_PROTOCOL_MAJOR: u16 = 1;
/// Current minor version of the persistent Python runner protocol.
pub const PYTHON_RUNNER_PROTOCOL_MINOR: u16 = 0;
/// Maximum JSON payload accepted by the length-prefixed control channel.
pub const PYTHON_RUNNER_MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

/// A separately negotiated runner protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PythonRunnerProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

impl PythonRunnerProtocolVersion {
    #[must_use]
    pub const fn current() -> Self {
        Self {
            major: PYTHON_RUNNER_PROTOCOL_MAJOR,
            minor: PYTHON_RUNNER_PROTOCOL_MINOR,
        }
    }

    pub fn negotiate(self, peer: Self) -> Result<Self, PythonProtocolError> {
        if self.major != peer.major {
            return Err(PythonProtocolError::ProtocolMajorMismatch {
                host: self.major,
                worker: peer.major,
            });
        }
        Ok(Self {
            major: self.major,
            minor: self.minor.min(peer.minor),
        })
    }
}

/// Optional protocol behavior advertised during the worker handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PythonRunnerFeature {
    Describe,
    Invoke,
    Cancel,
    Health,
    HostCalls,
}

/// Python implementation details supplied by the worker, never inferred by the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PythonRuntimeIdentity {
    pub implementation: String,
    pub version: String,
}

/// Worker-initiated handshake sent before any host request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PythonRunnerHello {
    pub protocol: PythonRunnerProtocolVersion,
    pub sdk_version: String,
    pub python: PythonRuntimeIdentity,
    #[serde(default)]
    pub features: Vec<PythonRunnerFeature>,
}

/// Return the deterministic feature intersection in host preference order.
#[must_use]
pub fn negotiate_runner_features(
    host: &[PythonRunnerFeature],
    worker: &[PythonRunnerFeature],
) -> Vec<PythonRunnerFeature> {
    host.iter()
        .copied()
        .filter(|feature| worker.contains(feature))
        .collect()
}

/// Trace context propagated with one invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PythonTraceContext {
    pub traceparent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracestate: Option<String>,
}

/// Authenticated actor and scopes captured when dispatch begins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PythonActorContext {
    pub actor_id: String,
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// Complete at-most-once invocation envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PythonInvocationRequest {
    pub invocation_id: String,
    pub provider: String,
    pub action: String,
    pub arguments: Value,
    pub deadline_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<PythonTraceContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<PythonActorContext>,
    pub cancellation_token_id: String,
    pub generation_id: String,
}

/// Requests sent by the Soma host over the dedicated runner control channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum PythonRunnerHostRequest {
    Describe {
        request_id: u64,
        path: PathBuf,
        generation_id: String,
    },
    Invoke {
        request_id: u64,
        invocation: Box<PythonInvocationRequest>,
    },
    Cancel {
        request_id: u64,
        invocation_id: String,
        cancellation_token_id: String,
    },
    Health {
        request_id: u64,
    },
    Drain {
        request_id: u64,
    },
    Shutdown {
        request_id: u64,
    },
}

/// Worker lifecycle visible to the supervisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PythonWorkerHealth {
    Starting,
    Ready,
    Draining,
    Unhealthy,
}

/// Stable invocation lifecycle used to decide whether retry is safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PythonInvocationState {
    Pending,
    Accepted,
    Running,
    Completed,
    Cancelled,
    Indeterminate,
}

/// Worker replies to host requests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PythonRunnerReply {
    Ok {
        request_id: u64,
        result: Value,
    },
    Accepted {
        request_id: u64,
        invocation_id: String,
        state: PythonInvocationState,
    },
    Health {
        request_id: u64,
        health: PythonWorkerHealth,
        generation_id: String,
    },
    Error {
        request_id: u64,
        error: PythonRunnerError,
    },
}

/// Brokered calls a worker may request from the host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method")]
pub enum PythonRunnerHostCall {
    #[serde(rename = "host.http")]
    Http {
        request_id: u64,
        invocation_id: String,
        request: Value,
    },
    #[serde(rename = "host.secret")]
    Secret {
        request_id: u64,
        invocation_id: String,
        name: String,
    },
    #[serde(rename = "host.state.get")]
    StateGet {
        request_id: u64,
        invocation_id: String,
        key: String,
    },
    #[serde(rename = "host.state.put")]
    StatePut {
        request_id: u64,
        invocation_id: String,
        key: String,
        value: Value,
    },
    #[serde(rename = "host.log")]
    Log {
        request_id: u64,
        invocation_id: String,
        level: String,
        message: String,
        #[serde(default)]
        fields: Value,
    },
    #[serde(rename = "host.metric")]
    Metric {
        request_id: u64,
        invocation_id: String,
        name: String,
        value: serde_json::Number,
        #[serde(default)]
        attributes: Value,
    },
    #[serde(rename = "host.progress")]
    Progress {
        request_id: u64,
        invocation_id: String,
        current: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

/// Stable public error identifiers for Python setup and execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PythonRunnerErrorCode {
    PythonRuntimeMissing,
    PythonVersionUnsupported,
    PythonDependencyResolutionFailed,
    PythonWorkerStartFailed,
    PythonProtocolMismatch,
    PythonCatalogTimeout,
    PythonImportFailed,
    PythonSchemaInvalid,
    PythonPolicyDenied,
    PythonCallTimeout,
    PythonCallCancelled,
    PythonWorkerCrashed,
    PythonOutputTooLarge,
    PythonInvalidOutput,
    PythonNativeAbiMismatch,
}

/// Stable stage at which a runner error occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PythonRunnerErrorPhase {
    Runtime,
    DependencyResolution,
    WorkerStartup,
    Protocol,
    Catalog,
    Import,
    Schema,
    Policy,
    Invocation,
    NativeBinding,
}

/// Redacted error envelope safe to cross the private runner protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PythonRunnerError {
    pub code: PythonRunnerErrorCode,
    pub phase: PythonRunnerErrorPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    pub retryable: bool,
    pub public_message: String,
}

#[derive(Debug, Error)]
pub enum PythonProtocolError {
    #[error("invalid Python worker JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported Python worker schema version {actual}; expected {expected}")]
    UnsupportedSchemaVersion { expected: u32, actual: u32 },
    #[error("unexpected Python worker request id {actual}; expected {expected}")]
    UnexpectedRequestId { expected: u64, actual: u64 },
    #[error("unexpected Python worker response mode {actual}; expected {expected}")]
    UnexpectedResponseMode {
        expected: &'static str,
        actual: &'static str,
    },
    #[error("Python runner protocol major mismatch: host {host}, worker {worker}")]
    ProtocolMajorMismatch { host: u16, worker: u16 },
    #[error("Python runner frame header is incomplete: got {actual} bytes; expected 4")]
    FrameHeaderTooShort { actual: usize },
    #[error("Python runner frame payload is {actual} bytes; limit is {limit}")]
    FrameTooLarge { limit: usize, actual: usize },
    #[error("Python runner frame length mismatch: declared {declared} bytes; got {actual}")]
    FrameLengthMismatch { declared: usize, actual: usize },
}

/// Encode one control message as a four-byte big-endian length plus UTF-8 JSON.
pub fn encode_runner_frame<T: Serialize>(message: &T) -> Result<Vec<u8>, PythonProtocolError> {
    let payload = serde_json::to_vec(message)?;
    if payload.len() > PYTHON_RUNNER_MAX_FRAME_BYTES {
        return Err(PythonProtocolError::FrameTooLarge {
            limit: PYTHON_RUNNER_MAX_FRAME_BYTES,
            actual: payload.len(),
        });
    }

    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

/// Decode exactly one complete control frame.
pub fn decode_runner_frame<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, PythonProtocolError> {
    if bytes.len() < 4 {
        return Err(PythonProtocolError::FrameHeaderTooShort {
            actual: bytes.len(),
        });
    }

    let declared = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    if declared > PYTHON_RUNNER_MAX_FRAME_BYTES {
        return Err(PythonProtocolError::FrameTooLarge {
            limit: PYTHON_RUNNER_MAX_FRAME_BYTES,
            actual: declared,
        });
    }

    let payload = &bytes[4..];
    if payload.len() != declared {
        return Err(PythonProtocolError::FrameLengthMismatch {
            declared,
            actual: payload.len(),
        });
    }

    Ok(serde_json::from_slice(payload)?)
}

pub(crate) fn encode_python_request(
    request: &PythonWorkerRequest,
) -> Result<Vec<u8>, PythonProtocolError> {
    validate_schema_version(request.schema_version())?;
    let mut encoded = serde_json::to_vec(request)?;
    encoded.push(b'\n');
    Ok(encoded)
}

pub(crate) fn decode_python_response(
    bytes: &[u8],
) -> Result<PythonWorkerResponse, PythonProtocolError> {
    let response: PythonWorkerResponse = serde_json::from_slice(bytes)?;
    validate_schema_version(response.schema_version())?;
    Ok(response)
}

pub(crate) fn validate_python_response(
    request: &PythonWorkerRequest,
    response: &PythonWorkerResponse,
) -> Result<(), PythonProtocolError> {
    validate_schema_version(request.schema_version())?;
    validate_schema_version(response.schema_version())?;
    if response.request_id() != request.request_id() {
        return Err(PythonProtocolError::UnexpectedRequestId {
            expected: request.request_id(),
            actual: response.request_id(),
        });
    }
    if response.mode() != request.mode() {
        return Err(PythonProtocolError::UnexpectedResponseMode {
            expected: request.mode(),
            actual: response.mode(),
        });
    }
    Ok(())
}

fn validate_schema_version(actual: u32) -> Result<(), PythonProtocolError> {
    if actual == PYTHON_WORKER_SCHEMA_VERSION {
        return Ok(());
    }
    Err(PythonProtocolError::UnsupportedSchemaVersion {
        expected: PYTHON_WORKER_SCHEMA_VERSION,
        actual,
    })
}

#[cfg(test)]
#[path = "python_protocol_tests.rs"]
mod tests;
