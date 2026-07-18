//! Generic CORS configuration.
//!
//! This builder takes origins, methods, and headers as parameters — it has
//! no opinion on which origins or headers a product allows. Product-specific
//! header lists (e.g. Soma's `mcp-protocol-version`) stay in the owning
//! product crate and get passed in here.

use axum::http::{HeaderName, HeaderValue, Method};
pub use tower_http::cors::CorsLayer;

/// Build a `CorsLayer` that allows exactly the given origins, methods, and
/// request headers. Callers own picking a sane (non-empty, non-wildcard when
/// credentials matter) set of origins.
pub fn cors_layer(
    origins: Vec<HeaderValue>,
    methods: Vec<Method>,
    headers: Vec<HeaderName>,
) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(methods)
        .allow_headers(headers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn preflight_from_an_allowed_origin_succeeds() {
        let origin: HeaderValue = "https://example.test".parse().unwrap();
        let layer = cors_layer(
            vec![origin.clone()],
            vec![Method::GET],
            vec![axum::http::header::CONTENT_TYPE],
        );
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(layer);

        let request = Request::builder()
            .method(Method::OPTIONS)
            .uri("/")
            .header(axum::http::header::ORIGIN, origin.clone())
            .header(axum::http::header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .unwrap(),
            &origin
        );
    }

    #[tokio::test]
    async fn preflight_from_a_disallowed_origin_omits_the_allow_origin_header() {
        // The whole point of `cors_layer` is restricting origins — a
        // misconfiguration that accidentally widens the allow-list is a
        // security regression, so assert the negative case explicitly
        // rather than only ever testing the happy path.
        let allowed: HeaderValue = "https://example.test".parse().unwrap();
        let disallowed: HeaderValue = "https://not-allowed.test".parse().unwrap();
        let layer = cors_layer(
            vec![allowed],
            vec![Method::GET],
            vec![axum::http::header::CONTENT_TYPE],
        );
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(layer);

        let request = Request::builder()
            .method(Method::OPTIONS)
            .uri("/")
            .header(axum::http::header::ORIGIN, disallowed)
            .header(axum::http::header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert!(
            response
                .headers()
                .get(axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none(),
            "a disallowed origin must not receive an Access-Control-Allow-Origin header"
        );
    }
}
