use std::sync::Arc;

use axum::{
    body::{Body, Bytes},
    http::{header, HeaderMap, Request, StatusCode},
    response::IntoResponse,
    routing::post,
};
use soma_gateway::config::{GatewayConfig, ProtectedMcpRouteConfig, UpstreamConfig};
use soma_runtime::server::{gateway_product_state_from_config, AppState, AuthPolicy};
use tokio::sync::Mutex;
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

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .expect("content-type should be set");
    assert!(content_type.starts_with("application/json"));
}

#[tokio::test]
async fn protected_route_metadata_uses_route_resource_and_scopes() {
    let temp = tempfile::tempdir().unwrap();
    let state = oauth_state_with_gateway(&temp, protected_gateway_config(None, None)).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/.well-known/oauth-protected-resource/media")
                .header(header::HOST, "mcp.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["resource"], "https://mcp.example.com/media");
    assert_eq!(json["scopes_supported"], serde_json::json!(["soma:read"]));
}

#[tokio::test]
async fn protected_route_missing_bearer_returns_route_challenge() {
    let temp = tempfile::tempdir().unwrap();
    let state = oauth_state_with_gateway(&temp, protected_gateway_config(None, None)).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/media")
                .header(header::HOST, "mcp.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let challenge = response
        .headers()
        .get(header::WWW_AUTHENTICATE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(
        challenge.contains("https://mcp.example.com/.well-known/oauth-protected-resource/media")
    );
    assert!(challenge.contains("scope=\"soma:read\""));
}

#[tokio::test]
async fn protected_route_proxy_strips_public_bearer_and_adds_upstream_auth() {
    let seen_auth = Arc::new(Mutex::new(Vec::new()));
    let backend = backend_server(seen_auth.clone()).await;
    std::env::set_var("SOMA_TEST_UPSTREAM_TOKEN", "Bearer upstream-secret");
    let temp = tempfile::tempdir().unwrap();
    let state = oauth_state_with_gateway(
        &temp,
        protected_gateway_config(Some(backend), Some("SOMA_TEST_UPSTREAM_TOKEN")),
    )
    .await;
    let token = protected_route_token(&state, "https://mcp.example.com/media", "soma:read");

    let response = router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/media")
                .header(header::HOST, "mcp.example.com")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"jsonrpc":"2.0","id":1}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    std::env::remove_var("SOMA_TEST_UPSTREAM_TOKEN");
    assert_eq!(response.status(), StatusCode::OK);
    let seen = seen_auth.lock().await;
    assert_eq!(seen.as_slice(), ["Bearer upstream-secret"]);
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

async fn oauth_state_with_gateway(temp: &tempfile::TempDir, gateway: GatewayConfig) -> AppState {
    let mut state = crate::testing::oauth_state(temp.path()).await;
    state.gateway = gateway_product_state_from_config(gateway).unwrap();
    state
}

fn protected_gateway_config(
    upstream_url: Option<String>,
    bearer_token_env: Option<&str>,
) -> GatewayConfig {
    GatewayConfig {
        upstream: upstream_url
            .map(|url| UpstreamConfig {
                name: "backend".to_owned(),
                url: Some(url),
                bearer_token_env: bearer_token_env.map(ToOwned::to_owned),
                ..UpstreamConfig::default()
            })
            .into_iter()
            .collect(),
        protected_mcp_routes: vec![ProtectedMcpRouteConfig {
            name: "media".to_owned(),
            public_host: "mcp.example.com".to_owned(),
            public_path: "/media".to_owned(),
            upstream: Some("backend".to_owned()),
            scopes: vec!["soma:read".to_owned()],
            ..ProtectedMcpRouteConfig::default()
        }],
        ..GatewayConfig::default()
    }
}

fn protected_route_token(state: &AppState, audience: &str, scope: &str) -> String {
    let AuthPolicy::Mounted {
        auth_state: Some(auth_state),
    } = &state.auth_policy
    else {
        panic!("test state must use OAuth auth policy");
    };
    auth_state
        .signing_keys
        .issue_access_token(&soma_auth::jwt::AccessClaims {
            iss: "https://example.example.com".to_owned(),
            sub: "google-user".to_owned(),
            aud: audience.to_owned(),
            exp: 4_102_444_800,
            iat: 1_700_000_000,
            jti: "protected-route-test".to_owned(),
            scope: scope.to_owned(),
            azp: "client".to_owned(),
        })
        .unwrap()
}

async fn backend_server(seen_auth: Arc<Mutex<Vec<String>>>) -> String {
    let app = axum::Router::new().route(
        "/mcp",
        post(move |headers: HeaderMap, _body: Bytes| {
            let seen_auth = seen_auth.clone();
            async move {
                if let Some(value) = headers
                    .get(header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                {
                    seen_auth.lock().await.push(value.to_owned());
                }
                (StatusCode::OK, "proxied").into_response()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/mcp")
}
