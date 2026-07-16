use crate::ToolError;

#[cfg(feature = "openapi")]
use serde_json::Value;

#[cfg(feature = "openapi")]
pub use soma_openapi::OpenApiRegistry;

#[cfg(feature = "openapi")]
#[must_use]
pub fn split_openapi_method(method: &str) -> Option<(&str, &str)> {
    let (label, operation_id) = method.split_once('.')?;
    if label.is_empty() || operation_id.is_empty() {
        return None;
    }
    Some((label, operation_id))
}

#[cfg(not(feature = "openapi"))]
#[must_use]
pub fn openapi_provider_unavailable_error() -> ToolError {
    ToolError::UnknownAction {
        message: "unknown Code Mode local provider `openapi`".to_string(),
        valid: vec!["state".to_string(), "git".to_string()],
        hint: None,
    }
}

#[cfg(feature = "openapi")]
pub async fn dispatch_openapi_provider(
    registry: &soma_openapi::OpenApiRegistry,
    client: &reqwest::Client,
    method: &str,
    params: Value,
) -> Result<Value, ToolError> {
    let (label, operation_id) =
        split_openapi_method(method).ok_or_else(|| ToolError::InvalidParam {
            message: "openapi call must be openapi::<label>.<operationId>".to_string(),
            param: "id".to_string(),
        })?;
    soma_openapi::dispatch_openapi_call(registry, client, label, operation_id, params)
        .await
        .map_err(openapi_error_to_tool_error)
}

#[cfg(feature = "openapi")]
#[must_use]
pub fn openapi_error_to_tool_error(error: soma_openapi::OpenApiError) -> ToolError {
    match error {
        soma_openapi::OpenApiError::UnknownInstance { label, valid } => {
            ToolError::UnknownInstance {
                message: format!("unknown OpenAPI spec label `{label}`"),
                valid,
            }
        }
        soma_openapi::OpenApiError::UnknownOperation {
            label,
            operation_id,
        } => ToolError::UnknownAction {
            message: format!("unknown OpenAPI operation `{operation_id}` in `{label}`"),
            valid: Vec::new(),
            hint: None,
        },
        soma_openapi::OpenApiError::InvalidPathParam { label, param } => ToolError::InvalidParam {
            message: format!(
                "OpenAPI operation `{label}` path parameter `{param}` is missing or invalid"
            ),
            param,
        },
        soma_openapi::OpenApiError::RequestBlockedPrivateAddr { .. } => ToolError::Forbidden {
            message: error.to_string(),
            required_scopes: vec!["openapi".to_string()],
        },
        soma_openapi::OpenApiError::UpstreamTimeout { .. } => ToolError::Sdk {
            sdk_kind: "timeout".to_string(),
            message: error.to_string(),
        },
        other => ToolError::Sdk {
            sdk_kind: other.kind().to_string(),
            message: other.to_string(),
        },
    }
}
