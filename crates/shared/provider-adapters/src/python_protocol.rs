use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
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

#[derive(Debug, Error)]
pub(crate) enum PythonProtocolError {
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
