//! Route-level tests for gateway REST dispatch and mounted auth behavior.
#![cfg(feature = "mcp-http")]

use axum::http::{Method, StatusCode};
use serde_json::json;
use soma::{
    server,
    testing::{bearer_state, loopback_state},
};
use soma_contracts::scopes::ADMIN_SCOPE;

mod support;
use support::request_json;

#[tokio::test]
async fn mounted_bearer_token_can_read_gateway_discovery() {
    let app = server::router(bearer_state("secret"));
    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/gateway/gateway.list",
        Some("secret"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["upstream_count"], 0);
}

#[tokio::test]
async fn mounted_bearer_token_cannot_call_gateway_admin_actions() {
    let app = server::router(bearer_state("secret"));
    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/gateway/gateway.test",
        Some("secret"),
        Some(json!({"command": "echo"})),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "admin_required");
}

#[tokio::test]
async fn loopback_can_call_gateway_admin_actions_without_mounted_auth() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(
        app.clone(),
        Method::POST,
        "/v1/gateway/gateway.remove",
        None,
        Some(json!({})),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "invalid_param");

    let (status, body) = request_json(
        app.clone(),
        Method::POST,
        "/v1/gateway/gateway.add",
        None,
        Some(json!({"name": "demo", "url": "https://example.com/mcp"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["added"], true);

    let (status, body) = request_json(
        app.clone(),
        Method::POST,
        "/v1/gateway/gateway.list",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["upstream_count"], 1);

    let (status, body) = request_json(
        app.clone(),
        Method::POST,
        "/v1/gateway/gateway.remove",
        None,
        Some(json!({"name": "demo"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["removed"], "demo");

    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/gateway/gateway.typo",
        None,
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "unknown_action");
}

#[test]
fn oauth_admin_scope_is_available_to_gateway_policy() {
    assert_eq!(ADMIN_SCOPE, "soma:admin");
}
