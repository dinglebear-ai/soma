//! Generic per-request timeout.
//!
//! Unlike plain `tower::timeout`, `tower_http`'s [`TimeoutLayer`] never
//! fails the inner service — on elapse it directly returns a response with
//! a caller-chosen status code, so it composes onto an Axum `Router` with
//! no error-handling layer required. `tower_http` itself has no built-in
//! default status code (its zero-arg constructor is deprecated in favor of
//! `with_status_code`); [`timeout_layer`] below is this crate's own choice
//! to default that code to `408 Request Timeout`.

use std::time::Duration;

use axum::http::StatusCode;
pub use tower_http::timeout::TimeoutLayer;

/// Build a layer that returns `408 Request Timeout` (this crate's chosen
/// default — see the module docs) for any request taking longer than
/// `duration`.
pub fn timeout_layer(duration: Duration) -> TimeoutLayer {
    TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, duration)
}

/// Build a layer that returns `status_code` for any request taking longer
/// than `duration`.
pub fn timeout_layer_with_status(status_code: StatusCode, duration: Duration) -> TimeoutLayer {
    TimeoutLayer::with_status_code(status_code, duration)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::get, Router};
    use tower::ServiceExt;

    #[tokio::test]
    async fn fast_handler_is_unaffected_by_a_generous_timeout() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(timeout_layer(Duration::from_secs(30)));

        let request = Request::builder().uri("/").body(Body::empty()).unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn slow_handler_is_cut_off_by_the_timeout() {
        let app = Router::new()
            .route(
                "/",
                get(|| async {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    "too slow"
                }),
            )
            .layer(timeout_layer(Duration::from_millis(20)));

        let request = Request::builder().uri("/").body(Body::empty()).unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
    }
}
