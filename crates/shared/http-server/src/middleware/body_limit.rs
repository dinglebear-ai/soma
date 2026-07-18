//! Generic request body size limit.

pub use tower_http::limit::RequestBodyLimitLayer;

/// Build a layer that rejects request bodies larger than `bytes`.
pub fn body_limit_layer(bytes: usize) -> RequestBodyLimitLayer {
    RequestBodyLimitLayer::new(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, body::Bytes, http::Request, routing::post, Router};
    use tower::ServiceExt;

    // Handlers extract `Bytes` so the body is actually streamed and read —
    // `RequestBodyLimitLayer` enforces its limit while the body is
    // consumed, so a handler that ignores the body entirely would never
    // observe the rejection.

    #[tokio::test]
    async fn request_within_limit_is_accepted() {
        let app = Router::new()
            .route(
                "/",
                post(|body: Bytes| async move { body.len().to_string() }),
            )
            .layer(body_limit_layer(16));

        let request = Request::builder()
            .method("POST")
            .uri("/")
            .body(Body::from("small"))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn request_exactly_at_limit_is_accepted() {
        // Boundary check: a body of exactly `bytes` must be accepted, not
        // rejected — guards against an off-by-one (`>` vs `>=`) regression
        // in the limit comparison.
        let app = Router::new()
            .route(
                "/",
                post(|body: Bytes| async move { body.len().to_string() }),
            )
            .layer(body_limit_layer(8));

        let request = Request::builder()
            .method("POST")
            .uri("/")
            .body(Body::from("exactly8"))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn request_over_limit_is_rejected() {
        let app = Router::new()
            .route(
                "/",
                post(|body: Bytes| async move { body.len().to_string() }),
            )
            .layer(body_limit_layer(4));

        let request = Request::builder()
            .method("POST")
            .uri("/")
            .body(Body::from("this body is far too large"))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);
    }
}
