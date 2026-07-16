use serde_json::Value;
use thiserror::Error;

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
    let parsed = url::Url::parse(url).map_err(|_| OpenApiAdapterError::SpecUrlDenied)?;
    soma_openapi::ssrf::validate_spec_url("gateway", &parsed)
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
