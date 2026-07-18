//! Reusable JSON response envelope and error-body helpers.

use axum::{
    extract::rejection::JsonRejection,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

use crate::problem::ErrorBody;

/// Render a JSON body extraction failure as a response.
///
/// `413 Payload Too Large` when the body exceeded the configured limit,
/// `400 Bad Request` for every other rejection (missing/invalid content
/// type, malformed JSON, etc.).
pub fn json_rejection_response(error: JsonRejection) -> Response {
    let status = if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
        StatusCode::PAYLOAD_TOO_LARGE
    } else {
        StatusCode::BAD_REQUEST
    };
    ErrorBody::new(error.to_string()).into_response_with_status(status)
}

/// Render a `400 Bad Request` with a generic validation error body.
pub fn validation_error_response(message: impl Into<String>) -> Response {
    ErrorBody::new("validation_error")
        .with_message(message)
        .into_response_with_status(StatusCode::BAD_REQUEST)
}

/// Render any `Serialize` payload as a JSON response with the given status.
pub fn json_response(status: StatusCode, body: impl Serialize) -> Response {
    (status, Json(body)).into_response()
}

/// Map a Soma `ApplicationError.code` value to the HTTP status it renders
/// as.
///
/// This is the single, shared classification used across every REST/HTTP
/// product surface that emits `ApplicationError` bodies (`soma-api`,
/// `soma-palette`, ...). Product-surface crates must not depend on one
/// another (see `xtask check-architecture`), so this table lives here
/// rather than being duplicated per surface or exposed by one surface for
/// another to import. Callers pass the plain `code` string rather than the
/// `ApplicationError` type itself — that type lives in `soma-application`
/// (a `crates/soma/*` product crate), and `shared/*` crates must never
/// depend on `crates/soma/*` or `apps/*`.
pub fn application_error_status(code: &str) -> StatusCode {
    match code {
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
mod tests {
    use axum::{
        body::{to_bytes, Body},
        extract::DefaultBodyLimit,
        http::Request,
        routing::post,
        Router,
    };
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn validation_error_response_is_bad_request_with_message() {
        let response = validation_error_response("name is required");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["error"], "validation_error");
        assert_eq!(value["message"], "name is required");
    }

    #[tokio::test]
    async fn json_response_serializes_status_and_body() {
        let response = json_response(StatusCode::CREATED, serde_json::json!({"ok": true}));
        assert_eq!(response.status(), StatusCode::CREATED);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["ok"], true);
    }

    #[test]
    fn application_error_status_maps_known_codes() {
        assert_eq!(
            application_error_status("unknown_action"),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            application_error_status("capability_denied"),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            application_error_status("input_too_large"),
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(
            application_error_status("invalid_param"),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            application_error_status("unsupported_transport"),
            StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            application_error_status("engine_unavailable"),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn application_error_status_defaults_to_internal_server_error() {
        assert_eq!(
            application_error_status("something_unmapped"),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    /// Drive a real oversized-body rejection through a minimal Axum router
    /// (rather than hand-constructing a `JsonRejection`, which is
    /// `#[non_exhaustive]`) to cover `json_rejection_response`'s
    /// `PAYLOAD_TOO_LARGE` branch — the only conditional in this function,
    /// otherwise untested at this crate's layer (see `apps/soma/tests/
    /// api_routes.rs::oversized_body_returns_413` for the equivalent
    /// full-router integration test this unit test complements).
    #[tokio::test]
    async fn json_rejection_response_maps_oversized_body_to_413() {
        async fn handler(body: Result<Json<serde_json::Value>, JsonRejection>) -> Response {
            match body {
                Ok(_) => StatusCode::OK.into_response(),
                Err(error) => json_rejection_response(error),
            }
        }

        let app = Router::new()
            .route("/", post(handler))
            .layer(DefaultBodyLimit::max(16));
        let request = Request::builder()
            .method("POST")
            .uri("/")
            .header("content-type", "application/json")
            .body(Body::from(vec![b'x'; 100]))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn json_rejection_response_maps_malformed_json_to_400() {
        async fn handler(body: Result<Json<serde_json::Value>, JsonRejection>) -> Response {
            match body {
                Ok(_) => StatusCode::OK.into_response(),
                Err(error) => json_rejection_response(error),
            }
        }

        let app = Router::new().route("/", post(handler));
        let request = Request::builder()
            .method("POST")
            .uri("/")
            .header("content-type", "application/json")
            .body(Body::from("not json"))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
