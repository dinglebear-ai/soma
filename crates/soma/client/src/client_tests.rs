//! Unit tests for SomaClient — sidecar file for src/client.rs
//!
//! # Sidecar test pattern
//!
//! Tests live in a separate `*_tests.rs` file (this file) rather than inline in
//! `client.rs`. The parent module declares them with:
//!
//! ```rust
//! #[cfg(test)]
//! #[path = "client_tests.rs"]
//! mod tests;
//! ```
//!
//! Benefits of the sidecar pattern:
//!   - `client.rs` stays focused on production code — no test boilerplate
//!   - Tests can be found quickly (always `<module>_tests.rs`)
//!   - Large test suites don't inflate the source file line count
//!   - IDE navigation: open `client.rs`, jump to `mod tests`, find the file
//!
//! **Customize**: Copy this pattern for every module that needs unit tests.
//!   1. Create `src/<module>_tests.rs`
//!   2. Add `#[cfg(test)] #[path = "<module>_tests.rs"] mod tests;` to `src/<module>.rs`
//!   3. Write tests here — they can access `pub(crate)` items via `super::*`

use super::*;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode, Uri},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use soma_config::{RuntimeMode, SomaConfig};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

/// Helper: build a stub SomaConfig.
/// Tests do not make real network calls unless they opt into a mock deployed API.
fn stub_config() -> SomaConfig {
    SomaConfig {
        // Empty URL selects offline stub mode — safe for offline tests.
        // CUSTOMIZE: Replace with your service's config struct fields.
        api_url: String::new(),
        api_key: "test-key".to_string(),
        ..SomaConfig::default()
    }
}

#[tokio::test]
async fn test_greet_returns_greeting_field() {
    // CUSTOMIZE: Replace greet() with your client's first operation.
    //           Test that the response has the expected JSON shape.
    let client = SomaClient::new(&stub_config()).expect("stub client should build");
    let result = client.greet(None).await.expect("greet should succeed");

    assert!(
        result.get("greeting").is_some(),
        "greet response should have 'greeting' field, got: {result}"
    );
}

#[tokio::test]
async fn test_greet_with_name_includes_name_in_response() {
    // CUSTOMIZE: Test that input parameters are reflected in the output.
    //           This is a semantic test — not just "did it return JSON" but
    //           "did it return the RIGHT JSON for this input".
    let client = SomaClient::new(&stub_config()).expect("stub client should build");
    let result = client
        .greet(Some("Alice"))
        .await
        .expect("greet Alice should succeed");

    let greeting = result
        .get("greeting")
        .and_then(|v| v.as_str())
        .expect("greeting field should be a string");

    assert!(
        greeting.contains("Alice"),
        "greeting should include the provided name 'Alice', got: {greeting}"
    );
}

#[tokio::test]
async fn test_greet_default_name_is_world() {
    // CUSTOMIZE: Test default/fallback behavior explicitly.
    let client = SomaClient::new(&stub_config()).expect("stub client should build");
    let result = client.greet(None).await.expect("greet should succeed");

    let target = result
        .get("target")
        .and_then(|v| v.as_str())
        .expect("target field should be present");

    assert_eq!(target, "World", "default greeting target should be 'World'");
}

#[tokio::test]
async fn test_echo_returns_exact_message() {
    // CUSTOMIZE: For operations that pass data through, verify the round-trip exactly.
    //           "is it JSON?" is not a good test. "does it contain the right value?" is.
    let client = SomaClient::new(&stub_config()).expect("stub client should build");
    let message = "hello from the test suite";
    let result = client.echo(message).await.expect("echo should succeed");

    let echo = result
        .get("echo")
        .and_then(|v| v.as_str())
        .expect("echo field should be present");

    assert_eq!(echo, message, "echo should return the exact input message");
}

#[tokio::test]
async fn test_status_returns_ok() {
    // CUSTOMIZE: Status/health operations should always return a known good value.
    let client = SomaClient::new(&stub_config()).expect("stub client should build");
    let result = client.status().await.expect("status should succeed");

    let status = result
        .get("status")
        .and_then(|v| v.as_str())
        .expect("status field should be present");

    assert_eq!(status, "ok");
}

#[test]
fn test_client_builds_with_empty_config() {
    // CUSTOMIZE: Verify that the client can be constructed even with empty credentials.
    //           In Soma, this is intentional (the stub allows it).
    //           A real server would error here — update this test to expect an Err.
    let config = SomaConfig {
        api_url: String::new(),
        api_key: String::new(),
        ..SomaConfig::default()
    };
    let result = SomaClient::new(&config);
    // CUSTOMIZE: Change to assert!(result.is_err()) once you add real validation
    assert!(
        result.is_ok(),
        "stub client should build even with empty config (real server should validate)"
    );
}

#[test]
fn test_api_action_url_preserves_base_path() {
    let root = Url::parse("https://example.test/").unwrap();
    let nested = Url::parse("https://example.test/api").unwrap();
    let nested_slash = Url::parse("https://example.test/api/").unwrap();

    assert_eq!(
        api_url(&root, "v1/status").unwrap().as_str(),
        "https://example.test/v1/status"
    );
    assert_eq!(
        api_url(&nested, "v1/echo").unwrap().as_str(),
        "https://example.test/api/v1/echo"
    );
    assert_eq!(
        api_url(&nested_slash, "v1/greet").unwrap().as_str(),
        "https://example.test/api/v1/greet"
    );
}

#[tokio::test]
async fn test_client_forwards_actions_to_deployed_api_when_configured() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let (base_url, handle) = mock_deployed_api(observed.clone()).await;

    let client = SomaClient::new(&SomaConfig {
        api_url: base_url,
        api_key: "secret-token".to_string(),
        ..SomaConfig::default()
    })
    .expect("remote client should build");

    let greeting = client
        .greet(Some("Ada"))
        .await
        .expect("remote greet should succeed");
    let echo = client
        .echo("hello")
        .await
        .expect("remote echo should succeed");
    let status = client.status().await.expect("remote status should succeed");

    handle.abort();

    assert_eq!(greeting["source"], "deployed-api");
    assert_eq!(greeting["greeting"], "Hello, Ada!");
    assert_eq!(echo["echo"], "hello");
    assert_eq!(status["status"], "remote-ok");

    let observed = observed.lock().expect("observed requests should lock");
    assert_eq!(observed.len(), 3);
    assert!(observed
        .iter()
        .all(|request| request.bearer == "Bearer secret-token"));
    assert_eq!(observed[0].path, "/v1/greet");
    assert_eq!(observed[0].body["name"], "Ada");
    assert_eq!(observed[1].path, "/v1/echo");
    assert_eq!(observed[1].body["message"], "hello");
    assert_eq!(observed[2].path, "/v1/status");
    assert!(observed[2].body.is_null());
}

#[tokio::test]
async fn explicit_local_runtime_mode_ignores_configured_api_url() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let (base_url, handle) = mock_deployed_api(observed.clone()).await;

    let client = SomaClient::new(&SomaConfig {
        api_url: base_url,
        api_key: "secret-token".to_string(),
        runtime_mode: RuntimeMode::Local,
    })
    .expect("local client should build");

    let greeting = client
        .greet(Some("Ada"))
        .await
        .expect("local greet should use stub");

    handle.abort();

    assert_eq!(greeting["target"], "Ada");
    assert_eq!(greeting["server"], "");
    assert!(
        observed
            .lock()
            .expect("observed requests should lock")
            .is_empty(),
        "explicit local mode must not call the configured API URL"
    );
}

#[tokio::test]
async fn remote_provider_action_uses_catalog_generic_route() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let (base_url, handle) = mock_deployed_api(observed.clone()).await;

    let client = SomaClient::new(&SomaConfig {
        api_url: base_url,
        api_key: "secret-token".to_string(),
        runtime_mode: RuntimeMode::Remote,
    })
    .expect("remote client should build");

    let output = client
        .call_rest_action("weather-current", json!({"city": "Paris"}))
        .await
        .expect("remote provider action should succeed");

    handle.abort();

    assert_eq!(output["ok"], true);
    let observed = observed.lock().expect("observed requests should lock");
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].path, "/v1/tools/weather_current");
    assert_eq!(observed[0].body["city"], "Paris");
}

#[tokio::test]
async fn remote_provider_action_uses_catalog_custom_route() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let (base_url, handle) = mock_deployed_api(observed.clone()).await;

    let client = SomaClient::new(&SomaConfig {
        api_url: base_url,
        api_key: "secret-token".to_string(),
        runtime_mode: RuntimeMode::Remote,
    })
    .expect("remote client should build");

    let output = client
        .call_rest_action("ai-sdk-brief", json!({"text": "hello"}))
        .await
        .expect("remote provider action should succeed");

    handle.abort();

    assert_eq!(output["ok"], true);
    let observed = observed.lock().expect("observed requests should lock");
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].path, "/v1/providers/ai-sdk-brief");
    assert_eq!(observed[0].body["text"], "hello");
}

#[tokio::test]
async fn ready_is_ok_when_target_is_stub() {
    // Stub mode has no upstream, so readiness is trivially satisfied without
    // making a network call.
    let client = SomaClient::new(&stub_config()).expect("stub client should build");
    client.ready().await.expect("stub client should be ready");
}

#[tokio::test]
async fn ready_succeeds_when_upstream_health_is_ok() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let (base_url, handle) = mock_deployed_api(observed).await;

    let client = SomaClient::new(&SomaConfig {
        api_url: base_url,
        api_key: "secret-token".to_string(),
        ..SomaConfig::default()
    })
    .expect("remote client should build");

    let result = client.ready().await;
    handle.abort();

    result.expect("readiness probe should succeed when /health returns 2xx");
}

#[tokio::test]
async fn ready_fails_when_upstream_health_is_not_ok() {
    let (base_url, handle) =
        mock_fixed_response(StatusCode::SERVICE_UNAVAILABLE, "unavailable").await;

    let client = SomaClient::new(&SomaConfig {
        api_url: base_url,
        api_key: "secret-token".to_string(),
        ..SomaConfig::default()
    })
    .expect("remote client should build");

    let result = client.ready().await;
    handle.abort();

    let err = result.expect_err("readiness probe should fail on non-2xx /health response");
    assert!(
        err.to_string().contains("upstream not ready"),
        "error should explain readiness failure, got: {err}"
    );
}

#[tokio::test]
async fn call_deployed_api_errors_on_non_success_status() {
    let (base_url, handle) = mock_fixed_response(StatusCode::INTERNAL_SERVER_ERROR, "boom").await;

    let client = SomaClient::new(&SomaConfig {
        api_url: base_url,
        api_key: "secret-token".to_string(),
        ..SomaConfig::default()
    })
    .expect("remote client should build");

    let result = client.status().await;
    handle.abort();

    let err = result.expect_err("non-2xx deployed API response should error");
    assert!(
        err.to_string().contains("HTTP 500"),
        "error should surface the HTTP status, got: {err}"
    );
}

#[tokio::test]
async fn call_deployed_api_errors_on_invalid_json_body() {
    let (base_url, handle) = mock_fixed_response(StatusCode::OK, "not json").await;

    let client = SomaClient::new(&SomaConfig {
        api_url: base_url,
        api_key: "secret-token".to_string(),
        ..SomaConfig::default()
    })
    .expect("remote client should build");

    let result = client.status().await;
    handle.abort();

    let err = result.expect_err("non-JSON deployed API body should error");
    assert!(
        err.to_string().contains("invalid JSON"),
        "error should explain the JSON decode failure, got: {err}"
    );
}

#[test]
fn validate_action_path_segment_rejects_empty_action() {
    let err = validate_action_path_segment("").expect_err("empty action should fail validation");
    assert!(err.to_string().contains("non-empty path segment"));
}

#[test]
fn validate_action_path_segment_rejects_path_separators() {
    let err = validate_action_path_segment("a/b")
        .expect_err("action containing '/' should fail validation");
    assert!(err.to_string().contains("non-empty path segment"));
}

#[test]
fn validate_action_path_segment_accepts_plain_action() {
    validate_action_path_segment("greet").expect("plain action name should validate");
}

#[tokio::test]
async fn call_rest_action_rejects_invalid_action_before_any_network_call() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let (base_url, handle) = mock_deployed_api(observed.clone()).await;

    let client = SomaClient::new(&SomaConfig {
        api_url: base_url,
        api_key: "secret-token".to_string(),
        runtime_mode: RuntimeMode::Remote,
    })
    .expect("remote client should build");

    let result = client.call_rest_action("a/b", json!({})).await;
    handle.abort();

    result.expect_err("action containing '/' should be rejected before any request is made");
    assert!(
        observed
            .lock()
            .expect("observed requests should lock")
            .is_empty(),
        "invalid action should short-circuit before reaching the network"
    );
}

#[test]
fn remote_provider_route_rejects_non_rest_exposed_tool() {
    let catalog = json!({
        "providers": [{
            "name": "remote-tools",
            "tools": [{
                "name": "internal-only",
                "surfaces": { "mcp": true, "rest": false, "cli": true }
            }]
        }]
    });

    let err = remote_provider_route(&catalog, "internal-only")
        .expect_err("non-REST-exposed tool should error instead of silently routing");
    assert!(
        err.to_string().contains("not REST-exposed"),
        "error should explain why the route was rejected, got: {err}"
    );
}

#[derive(Debug, Clone)]
struct ObservedRequest {
    path: String,
    body: Value,
    bearer: String,
}

type ObservedRequests = Arc<Mutex<Vec<ObservedRequest>>>;

async fn mock_deployed_api(
    observed: ObservedRequests,
) -> (String, tokio::task::JoinHandle<std::io::Result<()>>) {
    let app = Router::new()
        .route("/v1/greet", post(mock_greet))
        .route("/v1/echo", post(mock_echo))
        .route("/v1/status", get(mock_status))
        .route("/v1/providers", get(mock_provider_catalog))
        .route("/v1/tools/weather_current", post(mock_provider_tool))
        .route("/v1/providers/ai-sdk-brief", post(mock_provider_tool))
        .route("/health", get(mock_health_ok))
        .with_state(observed);
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock API should bind");
    let addr = listener.local_addr().expect("mock API should have addr");
    let handle = tokio::spawn(async move { axum::serve(listener, app.into_make_service()).await });
    (format!("http://{addr}/"), handle)
}

async fn mock_greet(
    State(observed): State<ObservedRequests>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    push_observed(&observed, &headers, "/v1/greet", body.clone());
    Json(json!({
        "source": "deployed-api",
        "greeting": format!("Hello, {}!", body["name"].as_str().unwrap_or("World")),
    }))
}

async fn mock_echo(
    State(observed): State<ObservedRequests>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    push_observed(&observed, &headers, "/v1/echo", body.clone());
    Json(json!({ "echo": body["message"] }))
}

async fn mock_status(State(observed): State<ObservedRequests>, headers: HeaderMap) -> Json<Value> {
    push_observed(&observed, &headers, "/v1/status", Value::Null);
    Json(json!({ "status": "remote-ok" }))
}

async fn mock_health_ok() -> StatusCode {
    StatusCode::OK
}

/// Mock server that returns a fixed HTTP status and raw body for every path —
/// used for exercising `call_deployed_api_method`'s non-success-status and
/// invalid-JSON-body error branches without needing per-route wiring.
async fn mock_fixed_response(
    status: StatusCode,
    body: &'static str,
) -> (String, tokio::task::JoinHandle<std::io::Result<()>>) {
    async fn handler(
        State((status, body)): State<(StatusCode, &'static str)>,
    ) -> impl IntoResponse {
        (status, body)
    }

    let app = Router::new()
        .route("/{*path}", get(handler).post(handler))
        .route("/health", get(handler))
        .with_state((status, body));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock API should bind");
    let addr = listener.local_addr().expect("mock API should have addr");
    let handle = tokio::spawn(async move { axum::serve(listener, app.into_make_service()).await });
    (format!("http://{addr}/"), handle)
}

async fn mock_provider_catalog() -> Json<Value> {
    Json(json!({
        "schema_version": 1,
        "providers": [{
            "name": "remote-tools",
            "kind": "ai-sdk",
            "enabled": true,
            "tools": [
                {
                    "name": "weather_current",
                    "description": "Fetch current weather.",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "city": { "type": "string" }
                        }
                    },
                    "surfaces": { "mcp": true, "rest": true, "cli": true },
                    "cli": { "enabled": true, "command": "weather-current" },
                    "generic_rest": {
                        "enabled": true,
                        "method": "POST",
                        "path": "/v1/tools/weather_current"
                    }
                },
                {
                    "name": "ai_sdk_brief",
                    "description": "Create a brief.",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "text": { "type": "string" }
                        }
                    },
                    "surfaces": { "mcp": true, "rest": true, "cli": true },
                    "cli": { "enabled": true, "command": "ai-sdk-brief" },
                    "rest": {
                        "enabled": true,
                        "method": "POST",
                        "path": "/v1/providers/ai-sdk-brief",
                        "tags": [],
                        "deprecated": false,
                        "path_params": {},
                        "query_params": {}
                    },
                    "generic_rest": {
                        "enabled": true,
                        "method": "POST",
                        "path": "/v1/tools/ai_sdk_brief"
                    }
                }
            ],
            "prompts": [],
            "resources": []
        }]
    }))
}

async fn mock_provider_tool(
    State(observed): State<ObservedRequests>,
    uri: Uri,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Json<Value> {
    push_observed(&observed, &headers, uri.path(), body.clone());
    Json(json!({ "ok": true }))
}

fn push_observed(observed: &ObservedRequests, headers: &HeaderMap, path: &str, body: Value) {
    let bearer = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    observed
        .lock()
        .expect("observed requests should lock")
        .push(ObservedRequest {
            path: path.to_owned(),
            body,
            bearer,
        });
}
