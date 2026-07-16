use anyhow::Result;
use axum::{
    extract::rejection::JsonRejection,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use soma_contracts::token_limit::MAX_RESPONSE_BYTES;
use soma_service::{classify_service_error, ProviderError};

pub(crate) fn rest_error_response(error: anyhow::Error, action: &str) -> Response {
    let tool_error = classify_service_error(&error);
    if tool_error.kind == soma_contracts::errors::ServiceErrorKind::Validation {
        tracing::warn!(
            action = %action,
            code = %tool_error.code,
            "REST action rejected invalid params"
        );
    } else {
        tracing::error!(
            error = %error,
            action = %action,
            service_error_kind = %tool_error.kind.as_str(),
            "REST action execution failed"
        );
    }
    (
        StatusCode::from_u16(tool_error.http_status_code())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        Json(tool_error.to_rest_payload()),
    )
        .into_response()
}

pub(crate) fn rest_json_rejection_response(error: JsonRejection) -> Response {
    let status = if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
        StatusCode::PAYLOAD_TOO_LARGE
    } else {
        StatusCode::BAD_REQUEST
    };
    (status, Json(json!({"error": error.to_string()}))).into_response()
}

pub(crate) fn provider_rest_error_response(error: ProviderError) -> Response {
    let status = match &*error.code {
        "unknown_action" | "surface_not_exposed" => StatusCode::NOT_FOUND,
        "insufficient_scope" | "capability_denied" => StatusCode::FORBIDDEN,
        "input_too_large" | "response_too_large" => StatusCode::PAYLOAD_TOO_LARGE,
        "input_schema_failed" | "confirmation_required" => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let (provider, action, code) = error.log_code();
    tracing::warn!(provider, action, code, "REST provider call failed");
    (
        status,
        Json(serde_json::to_value(error).unwrap_or_else(|_| json!({"error":"provider_error"}))),
    )
        .into_response()
}

pub(crate) fn cap_rest_response(value: Value) -> Result<Value> {
    cap_json_response(
        value,
        "Use limit/offset parameters or more specific filters to get a smaller result.",
    )
}

pub(crate) fn cap_json_response(value: Value, hint: &'static str) -> Result<Value> {
    let serialized = serde_json::to_vec(&value)?;
    if serialized.len() <= MAX_RESPONSE_BYTES {
        return Ok(value);
    }
    Ok(json!({
        "truncated": true,
        "error": "response exceeded REST response size limit",
        "max_response_bytes": MAX_RESPONSE_BYTES,
        "hint": hint,
    }))
}

#[cfg(test)]
#[path = "responses_tests.rs"]
mod tests;
