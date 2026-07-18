//! Generic HTTP request tracing.
//!
//! A thin, named constructor over `tower_http`'s default HTTP trace layer so
//! callers don't have to spell out the classifier type. Products that want
//! custom spans (e.g. including the request ID) can still reach for
//! `tower_http::trace::TraceLayer` directly and layer it alongside
//! [`crate::middleware::request_id`].

use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::trace::TraceLayer;

/// Concrete type returned by [`trace_layer`] — spelled out once here so
/// callers don't need to name `tower_http`'s classifier generics themselves.
pub type HttpTraceLayer = TraceLayer<SharedClassifier<ServerErrorsAsFailures>>;

/// Build a `tower_http` request tracing layer with the library's default
/// HTTP classification (5xx responses and transport errors count as
/// failures; everything else is a success span).
pub fn trace_layer() -> HttpTraceLayer {
    TraceLayer::new_for_http()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::get, Router};
    use tower::ServiceExt;

    #[tokio::test]
    async fn trace_layer_passes_requests_through_unchanged() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(trace_layer());

        let request = Request::builder().uri("/").body(Body::empty()).unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }
}
