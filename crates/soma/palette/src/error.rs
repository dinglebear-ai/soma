//! Product error mapping for Palette UI responses.
//!
//! The Palette frontend renders `code`/`message`/`remediation` directly, so
//! the JSON body is `ApplicationError` itself (already `Serialize`); this
//! module only owns the HTTP status mapping. It delegates to
//! `soma-http-api`'s shared classification rather than duplicating
//! `soma-api`'s table — `product-surface` packages must not depend on one
//! another (see `xtask check-architecture`), so both surfaces depend on the
//! same `shared/*` crate instead.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use soma_application::ApplicationError;

pub fn palette_error_response(error: ApplicationError) -> Response {
    let status = palette_error_status(&error);
    tracing::warn!(code = %error.code, "palette request failed");
    (status, Json(error)).into_response()
}

pub fn palette_error_status(error: &ApplicationError) -> StatusCode {
    soma_http_api::response::application_error_status(&error.code)
}

/// `404` body for a launcher id that doesn't resolve to any palette-exposed
/// tool. The lookup happens in this crate, before any call reaches
/// `SomaApplication`, so there is no real `ApplicationError` to map — but
/// the body is still built as an `ApplicationError` value (rather than a
/// hand-rolled `json!` literal) so every `/v1/palette/*` error response,
/// found-then-failed or never-found, shares one wire shape
/// (`code`/`message`/`retryable`/`remediation`/`details`) for the frontend.
pub fn launcher_not_found(id: &str) -> Response {
    let error = ApplicationError::new(
        "launcher_not_found",
        format!("no palette-exposed launcher entry `{id}`"),
        false,
        "Refresh the catalog and use a known launcher id.",
    );
    (StatusCode::NOT_FOUND, Json(error)).into_response()
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
