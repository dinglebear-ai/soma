use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
};
use serde_json::{json, Value};
use soma_domain::AuthorizationMode;
use soma_provider_core::{ProviderId, ProviderManifest, ToolSpec};
use tower::ServiceExt;

use super::router;
use crate::state::PaletteState;

#[test]
fn router_registers_the_four_palette_routes() {
    // Building the router only registers routes; it does not require a live
    // `PaletteState` (that's only needed once `.with_state()` is called by
    // the composing app). This is a smoke test that route registration
    // itself does not panic and produces a router generic over
    // `PaletteState`, matching what apps/soma will mount.
    let _: axum::Router<crate::state::PaletteState> = router();
}

/// Build a live `axum::Router` with one fixture tool ("ping") registered,
/// so the four handlers can be exercised end-to-end through the real HTTP
/// stack (`tower::ServiceExt::oneshot`), matching the pattern
/// `apps/soma/tests/support.rs` uses for the REST surface.
fn app() -> axum::Router {
    let mut manifest = ProviderManifest::new(
        ProviderId::new("fixture").expect("valid provider id"),
        "fixture",
        "0.1.0",
    );
    manifest.tools = vec![ToolSpec::new("ping", "Ping", json!({"type": "object"}))];
    let application = soma_test_support::application_with_provider(manifest, json!({"pong": true}));
    let state = PaletteState::new(application, AuthorizationMode::LoopbackDev);
    router().with_state(state)
}

async fn send(
    app: axum::Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let request = if let Some(body) = body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        builder
            .body(Body::from(body.to_string()))
            .expect("request should build")
    } else {
        builder.body(Body::empty()).expect("request should build")
    };
    let response = app.oneshot(request).await.expect("route should respond");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("response body should be JSON")
    };
    (status, value)
}

#[tokio::test]
async fn get_catalog_returns_the_fixture_tool() {
    let (status, body) = send(app(), Method::GET, "/v1/palette/catalog", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["entries"][0]["id"], "ping");
    assert_eq!(body["schemaVersion"], 1);
}

#[tokio::test]
async fn get_search_filters_by_query() {
    let (status, body) = send(app(), Method::GET, "/v1/palette/search?q=ping", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["entries"].as_array().unwrap().len(), 1);

    let (status, body) = send(app(), Method::GET, "/v1/palette/search?q=nope", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["entries"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn get_schema_returns_schema_for_known_id() {
    let (status, body) = send(app(), Method::GET, "/v1/palette/schema?id=ping", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], "ping");
    assert_eq!(body["inputSchema"], json!({"type": "object"}));
}

#[tokio::test]
async fn get_schema_returns_404_for_unknown_id() {
    let (status, body) = send(app(), Method::GET, "/v1/palette/schema?id=missing", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "launcher_not_found");
}

#[tokio::test]
async fn post_execute_dispatches_known_id() {
    let request_body = json!({"id": "ping", "params": {}, "confirmDestructive": false});
    let (status, body) = send(
        app(),
        Method::POST,
        "/v1/palette/execute",
        Some(request_body),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["output"], json!({"pong": true}));
    assert!(body["requestId"].as_str().unwrap().starts_with("palette-"));
}

#[tokio::test]
async fn post_execute_returns_404_for_unknown_id() {
    let request_body = json!({"id": "missing", "params": {}});
    let (status, body) = send(
        app(),
        Method::POST,
        "/v1/palette/execute",
        Some(request_body),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "launcher_not_found");
}

#[tokio::test]
async fn post_execute_malformed_json_body_is_400_via_shared_rejection_renderer() {
    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/palette/execute")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{not json"))
        .expect("request should build");

    let response = app().oneshot(request).await.expect("route should respond");

    // Delegated to `soma_http_api::response::json_rejection_response`, so
    // malformed JSON is a 400 with the shared `ErrorBody` shape (`error`
    // carrying the parse-failure message), not the crate's own ad hoc
    // `{"error": ...}` literal.
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    let value: Value = serde_json::from_slice(&bytes).expect("response body should be JSON");
    assert!(value["error"].as_str().is_some_and(|s| !s.is_empty()));
}
