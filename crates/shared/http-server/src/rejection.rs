//! Generic rejection envelopes for routing-level failures: unmatched
//! routes and disallowed methods. Rendered through `soma_http_api`'s
//! structured error body so any HTTP surface that mounts these renders the
//! same shape for the same failure class. Body-extraction rejections
//! (malformed JSON, payload too large) are
//! `soma_http_api::response::json_rejection_response` — an API-shape
//! concern, not a transport concern, so they live there instead.
//!
//! [`not_found_handler`] is wired into `apps/soma`'s router `.fallback()`.
//! [`method_not_allowed`] is available for a product router to wire up (via
//! Axum's per-route method fallback) but is not currently mounted anywhere
//! — Axum's `.fallback()` only intercepts unmatched *paths*, not
//! matched-path/disallowed-method requests, so a consumer must opt in
//! separately for 405s to use this envelope.

use axum::{http::StatusCode, response::Response};
use soma_http_api::problem::ErrorBody;

/// Render a `404 Not Found` body: `{"error": "not_found"}`.
pub fn not_found() -> Response {
    ErrorBody::new("not_found").into_response_with_status(StatusCode::NOT_FOUND)
}

/// Axum `.fallback()` handler wrapping [`not_found`]. Mount with
/// `router.fallback(not_found_handler)`.
pub async fn not_found_handler() -> Response {
    not_found()
}

/// Render a `405 Method Not Allowed` body:
/// `{"error": "method_not_allowed"}`.
pub fn method_not_allowed() -> Response {
    ErrorBody::new("method_not_allowed").into_response_with_status(StatusCode::METHOD_NOT_ALLOWED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_is_404_with_stable_error_code() {
        let response = not_found();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn method_not_allowed_is_405_with_stable_error_code() {
        let response = method_not_allowed();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn not_found_handler_matches_not_found_body() {
        use axum::body::to_bytes;

        let direct = to_bytes(not_found().into_body(), usize::MAX).await.unwrap();
        let via_handler = to_bytes(not_found_handler().await.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(direct, via_handler);
    }
}
