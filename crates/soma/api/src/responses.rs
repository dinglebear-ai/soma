use axum::{
    extract::rejection::JsonRejection,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use soma_application::ApplicationError;
use soma_http_api::response::json_rejection_response;

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

/// Delegates to `soma-http-api`'s generic body-rejection renderer — every
/// Soma REST handler in this crate shares the exact same 413/400 mapping.
pub(crate) fn rest_json_rejection_response(error: JsonRejection) -> Response {
    json_rejection_response(error)
}

pub(crate) fn application_error_response(error: ApplicationError) -> Response {
    let status = application_error_status(&error);
    tracing::warn!(code = %error.code, "REST application request failed");
    (status, Json(error)).into_response()
}

/// Delegates to `soma-http-api`'s shared `ApplicationError.code` → status
/// mapping — every Soma REST/HTTP surface that renders `ApplicationError`
/// bodies shares the exact same classification (see
/// `soma_http_api::response::application_error_status`).
pub(crate) fn application_error_status(error: &ApplicationError) -> StatusCode {
    soma_http_api::response::application_error_status(&error.code)
}

#[cfg(test)]
#[path = "responses_tests.rs"]
mod tests;
