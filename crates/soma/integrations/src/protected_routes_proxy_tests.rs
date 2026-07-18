use axum::http::StatusCode;
use soma_gateway::config::{GatewayConfig, ProtectedMcpRouteConfig, UpstreamConfig};
use soma_runtime::server::gateway_product_state_from_config;

use crate::test_support;

use super::{append_proxy_suffix, configured_bearer_token, protected_route_upstream_target};

#[test]
fn append_proxy_suffix_preserves_backend_base_and_query() {
    let mut url = reqwest::Url::parse("http://example.com/mcp").unwrap();

    append_proxy_suffix(&mut url, "/messages", Some("session=1"));

    assert_eq!(url.as_str(), "http://example.com/mcp/messages?session=1");
}

#[test]
fn configured_bearer_token_normalizes_optional_scheme() {
    std::env::set_var("SOMA_TEST_PROXY_TOKEN", "Bearer secret");

    assert_eq!(
        configured_bearer_token("SOMA_TEST_PROXY_TOKEN").as_deref(),
        Some("secret")
    );

    std::env::remove_var("SOMA_TEST_PROXY_TOKEN");
}

// --- `protected_route_upstream_target` branch coverage ---
//
// PR 18 review flagged this resolver as untested: backend_url vs upstream vs
// neither, upstream-not-found, upstream-missing-url, and
// unsupported-transport all previously had no direct test.

fn route_with(backend_url: &str, upstream: Option<&str>) -> ProtectedMcpRouteConfig {
    ProtectedMcpRouteConfig {
        name: "media".to_owned(),
        public_host: "mcp.example.com".to_owned(),
        public_path: "/media".to_owned(),
        backend_url: backend_url.to_owned(),
        upstream: upstream.map(ToOwned::to_owned),
        ..ProtectedMcpRouteConfig::default()
    }
}

fn app_state_with_upstreams(upstreams: Vec<UpstreamConfig>) -> soma_runtime::server::AppState {
    let gateway = gateway_product_state_from_config(GatewayConfig {
        upstream: upstreams,
        protected_mcp_routes: vec![test_support::route()],
        ..GatewayConfig::default()
    })
    .expect("gateway with upstreams should validate");
    test_support::mounted_app_state(gateway, None)
}

#[tokio::test]
async fn backend_url_target_is_used_directly_over_upstream_lookup() {
    let state = app_state_with_upstreams(vec![]);
    let route = route_with("http://backend.example.com/mcp", None);

    let (url, token, label) = protected_route_upstream_target(&state, &route)
        .await
        .expect("backend_url route should resolve");

    assert_eq!(url.as_str(), "http://backend.example.com/mcp");
    assert_eq!(token, None);
    assert_eq!(label, "backend_url");
}

#[tokio::test]
async fn missing_backend_url_and_upstream_is_rejected() {
    let state = app_state_with_upstreams(vec![]);
    let route = route_with("", None);

    let error = protected_route_upstream_target(&state, &route)
        .await
        .expect_err("route with no target should be rejected");

    assert_eq!(error.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn unknown_upstream_name_is_rejected_with_404() {
    let state = app_state_with_upstreams(vec![]);
    let route = route_with("", Some("ghost"));

    let error = protected_route_upstream_target(&state, &route)
        .await
        .expect_err("unregistered upstream should be rejected");

    assert_eq!(error.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn upstream_without_an_http_url_is_rejected() {
    // A real, valid upstream — just stdio-transport, not HTTP-reachable.
    let state = app_state_with_upstreams(vec![UpstreamConfig {
        name: "stdio-up".to_owned(),
        command: Some("node".to_owned()),
        ..UpstreamConfig::default()
    }]);
    let route = route_with("", Some("stdio-up"));

    let error = protected_route_upstream_target(&state, &route)
        .await
        .expect_err("stdio-only upstream should be rejected for HTTP proxying");

    assert_eq!(error.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn upstream_with_unsupported_scheme_is_rejected() {
    // `ws`/`wss` pass `UpstreamConfig::validate()` (valid MCP transport
    // schemes generally), but the protected-route HTTP proxy only forwards
    // http(s) — anything else must be rejected here, not silently attempted.
    let state = app_state_with_upstreams(vec![UpstreamConfig {
        name: "ws-up".to_owned(),
        url: Some("ws://example.com/mcp".to_owned()),
        ..UpstreamConfig::default()
    }]);
    let route = route_with("", Some("ws-up"));

    let error = protected_route_upstream_target(&state, &route)
        .await
        .expect_err("non-http(s) upstream transport should be rejected");

    assert_eq!(error.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn upstream_with_valid_http_url_and_no_auth_resolves() {
    let state = app_state_with_upstreams(vec![UpstreamConfig {
        name: "http-up".to_owned(),
        url: Some("http://backend.example.com/mcp".to_owned()),
        ..UpstreamConfig::default()
    }]);
    let route = route_with("", Some("http-up"));

    let (url, token, label) = protected_route_upstream_target(&state, &route)
        .await
        .expect("http upstream should resolve");

    assert_eq!(url.as_str(), "http://backend.example.com/mcp");
    assert_eq!(token, None);
    assert_eq!(label, "upstream:http-up");
}

#[tokio::test]
async fn upstream_with_bearer_token_env_resolves_the_configured_token() {
    std::env::set_var("SOMA_TEST_UPSTREAM_TOKEN", "secret-value");
    let state = app_state_with_upstreams(vec![UpstreamConfig {
        name: "http-up".to_owned(),
        url: Some("http://backend.example.com/mcp".to_owned()),
        bearer_token_env: Some("SOMA_TEST_UPSTREAM_TOKEN".to_owned()),
        ..UpstreamConfig::default()
    }]);
    let route = route_with("", Some("http-up"));

    let (_, token, _) = protected_route_upstream_target(&state, &route)
        .await
        .expect("http upstream with bearer_token_env should resolve");

    std::env::remove_var("SOMA_TEST_UPSTREAM_TOKEN");
    assert_eq!(token.as_deref(), Some("secret-value"));
}
