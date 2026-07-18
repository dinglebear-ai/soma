//! A minimal, product-neutral structured-error response body.
//!
//! Not a full RFC 7807 "problem+json" implementation — just the small
//! reusable subset every JSON API surface in this workspace already agrees
//! on: an `error` value and an optional human-readable `message`. Product-
//! specific error shapes (with retry hints, remediation text, scoped codes,
//! etc.) stay in the owning product crate and may `From`-convert into this
//! type at the response boundary.
//!
//! `error` is a short machine-readable code (e.g. `"validation_error"`) for
//! call sites that classify their own failures. For framework-generated
//! rejections with no natural short code — see `json_rejection_response` in
//! `response.rs` — it is the framework's raw rejection text instead; there
//! is no stable code to assign a body-parsing failure that didn't originate
//! from this crate's own validation logic.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// Generic JSON error body: `{"error": "...", "message": "..."}`.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorBody {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ErrorBody {
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            message: None,
        }
    }

    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Render as an Axum response with the given status code.
    pub fn into_response_with_status(self, status: StatusCode) -> Response {
        (status, Json(self)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_body_omits_message_when_unset() {
        let body = ErrorBody::new("bad_request");
        assert_eq!(
            serde_json::to_value(&body).unwrap(),
            serde_json::json!({ "error": "bad_request" })
        );
    }

    #[test]
    fn error_body_includes_message_when_set() {
        let body = ErrorBody::new("bad_request").with_message("field is required");
        assert_eq!(
            serde_json::to_value(&body).unwrap(),
            serde_json::json!({ "error": "bad_request", "message": "field is required" })
        );
    }
}
