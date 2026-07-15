use serde_json::Value;
use thiserror::Error;

use crate::security::ssrf::{validate_url, OutboundPolicy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApiOperationRef {
    pub namespace: String,
    pub operation_id: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OpenApiAdapterError {
    #[error("OpenAPI spec URL denied by SSRF policy")]
    SpecUrlDenied,
    #[error("params must be a JSON object")]
    ParamsMustBeObject,
}

pub fn validate_spec_url(url: &str) -> Result<(), OpenApiAdapterError> {
    validate_url(url, OutboundPolicy::StrictExternal)
        .map(|_| ())
        .map_err(|_| OpenApiAdapterError::SpecUrlDenied)
}

pub fn validate_operation_params(params: &Value) -> Result<(), OpenApiAdapterError> {
    if params.is_object() {
        return Ok(());
    }
    Err(OpenApiAdapterError::ParamsMustBeObject)
}

#[cfg(test)]
#[path = "openapi_tests.rs"]
mod tests;
