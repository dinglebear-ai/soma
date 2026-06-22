use axum::{body::Body, http::Request};
use tower::ServiceExt;

use super::router;

#[tokio::test]
async fn openapi_json_is_served_without_auth() {
    let response = router(crate::testing::loopback_state())
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .expect("content-type should be set");
    assert!(content_type.starts_with("application/json"));
}

#[tokio::test]
async fn cors_preflight_allows_mcp_protocol_headers() {
    let response = router(crate::testing::loopback_state())
        .oneshot(
            Request::builder()
                .method(axum::http::Method::OPTIONS)
                .uri("/mcp")
                .header(axum::http::header::ORIGIN, "http://127.0.0.1:40060")
                .header(axum::http::header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                .header(
                    axum::http::header::ACCESS_CONTROL_REQUEST_HEADERS,
                    "mcp-protocol-version",
                )
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    let allow_headers = response
        .headers()
        .get(axum::http::header::ACCESS_CONTROL_ALLOW_HEADERS)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();

    // Mcp-Protocol-Version (2025-06-18+) and the draft SEP-2243 headers must be
    // permitted so browser-based MCP clients survive CORS preflight.
    for required in [
        "mcp-protocol-version",
        "mcp-method",
        "mcp-name",
        "x-mcp-header",
    ] {
        assert!(
            allow_headers.contains(required),
            "CORS allow-headers must include {required}, got: {allow_headers:?}"
        );
    }
}
