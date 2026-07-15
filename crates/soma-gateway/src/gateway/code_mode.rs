use serde_json::Value;
use thiserror::Error;

pub mod catalog;
pub mod host;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeModeToolId {
    pub namespace: String,
    pub tool: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayCodeModeError {
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CodeModeError {
    #[error("tool id must be namespace::tool")]
    InvalidToolId,
    #[error("params must be a JSON object")]
    ParamsMustBeObject,
    #[error("{kind}: {message}")]
    Gateway { kind: String, message: String },
}

#[must_use]
pub fn namespace_tool_id(namespace: &str, tool: &str) -> String {
    format!("{namespace}::{tool}")
}

pub fn parse_namespace_tool_id(id: &str) -> Result<CodeModeToolId, CodeModeError> {
    let Some((namespace, tool)) = id.split_once("::") else {
        return Err(CodeModeError::InvalidToolId);
    };
    if namespace.is_empty() || tool.is_empty() || tool.contains("::") {
        return Err(CodeModeError::InvalidToolId);
    }
    Ok(CodeModeToolId {
        namespace: namespace.to_owned(),
        tool: tool.to_owned(),
    })
}

pub fn ensure_object_params(params: &Value) -> Result<(), CodeModeError> {
    if params.is_object() {
        return Ok(());
    }
    Err(CodeModeError::ParamsMustBeObject)
}

pub fn preserve_gateway_error(error: GatewayCodeModeError) -> CodeModeError {
    CodeModeError::Gateway {
        kind: error.kind,
        message: error.message,
    }
}

#[cfg(test)]
#[path = "code_mode_tests.rs"]
mod tests;
