use axum::{
    extract::rejection::JsonRejection,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use soma_application::ApplicationError;

pub(crate) fn rest_error_response(error: anyhow::Error, action: &str) -> Response {
    tracing::warn!(error = %error, action, "REST action rejected invalid params");
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": "validation_error",
            "message": error.to_string(),
            "action": action,
        })),
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

pub(crate) fn application_error_response(error: ApplicationError) -> Response {
    let status = application_error_status(&error);
    tracing::warn!(code = %error.code, "REST application request failed");
    (status, Json(error)).into_response()
}

pub(crate) fn application_error_status(error: &ApplicationError) -> StatusCode {
    match error.code.as_str() {
        "unknown_action" | "surface_not_exposed" | "upstream_missing" | "unknown_upstream" => {
            StatusCode::NOT_FOUND
        }
        "insufficient_scope" | "capability_denied" | "admin_required" | "not_exposed" => {
            StatusCode::FORBIDDEN
        }
        "input_too_large" | "response_too_large" => StatusCode::PAYLOAD_TOO_LARGE,
        "input_schema_failed"
        | "confirmation_required"
        | "invalid_param"
        | "spawn_validation_failed"
        | "upstream_exists"
        | "invalid_config" => StatusCode::BAD_REQUEST,
        "unsupported_transport" => StatusCode::NOT_IMPLEMENTED,
        "gateway_reloading"
        | "store_not_mounted"
        | "oauth_runtime_error"
        | "not_routable"
        | "upstream_connect_failed"
        | "upstream_call_failed"
        | "engine_unavailable" => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[cfg(test)]
#[path = "responses_tests.rs"]
mod tests;
