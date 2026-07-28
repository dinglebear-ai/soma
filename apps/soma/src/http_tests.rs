use std::sync::Arc;

#[cfg(feature = "oauth")]
use std::collections::BTreeMap;

use axum::{
    body::{Body, Bytes},
    http::{header, HeaderMap, Request, StatusCode},
    response::IntoResponse,
    routing::post,
};
use soma_gateway::config::{GatewayConfig, ProtectedMcpRouteConfig, UpstreamConfig};
use soma_runtime::server::{AppState, AuthPolicy};
use tokio::sync::Mutex;
use tower::ServiceExt;

#[cfg(feature = "oauth")]
use mcp_client::oauth::{UpstreamOAuthManager, UpstreamOAuthRuntime};

#[cfg(feature = "oauth")]
#[path = "http_tests_oauth_stubs.rs"]
mod oauth_stubs;

#[cfg(feature = "oauth")]
use oauth_stubs::{
    FakeOAuthManager, FakeOAuthProvider, RecordedAuthorizationCallback, RecordingOAuthManager,
};

use super::router;

#[test]
fn api_and_mcp_states_share_the_runtime_application() {
    let state = crate::testing::loopback_state();
    let api = super::api_state(&state);
    let mcp = crate::bootstrap::mcp_state_for_state(&state);

    assert!(std::ptr::eq(state.application(), api.application()));
    assert!(std::ptr::eq(state.application(), mcp.application()));
}

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
async fn oauth_admin_gateway_add_is_visible_to_protected_route_proxy() {
    let backend = backend_server(Arc::new(Mutex::new(Vec::new()))).await;
    let temp = tempfile::tempdir().unwrap();
    let state = oauth_state_with_gateway(&temp, protected_gateway_config(None, None)).await;
    let admin_token = protected_route_token(
        &state,
        "https://example.example.com/mcp",
        soma_domain::scopes::ADMIN_SCOPE,
    );
    let route_token = protected_route_token(&state, "https://mcp.example.com/media", "soma:read");
    let app = router(state);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/gateway/gateway.add")
                .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({"name": "backend", "url": backend}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/media")
                .header(header::HOST, "mcp.example.com")
                .header(header::AUTHORIZATION, format!("Bearer {route_token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"jsonrpc":"2.0","id":1}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, "proxied");
}

#[cfg(feature = "oauth")]
#[tokio::test]
async fn upstream_oauth_state_is_shared_by_gateway_actions_and_protected_proxy() {
    let seen_auth = Arc::new(Mutex::new(Vec::new()));
    let backend = backend_server(seen_auth.clone()).await;
    let mut gateway_config = protected_gateway_config(Some(backend), None);
    gateway_config.upstream[0].oauth = Some(soma_gateway::config::GatewayUpstreamOauthConfig {
        mode: soma_gateway::config::GatewayUpstreamOauthMode::AuthorizationCodePkce,
        registration: soma_gateway::config::GatewayUpstreamOauthRegistration::Preregistered {
            client_id: "test-client".to_owned(),
            client_secret_env: None,
        },
        scopes: None,
        prefer_client_metadata_document: None,
    });
    let gateway = soma_runtime::server::gateway_product_state_from_config(gateway_config).unwrap();
    let mut managers: BTreeMap<String, Arc<dyn UpstreamOAuthManager>> = BTreeMap::new();
    managers.insert("backend".to_owned(), Arc::new(FakeOAuthManager));
    gateway.install_upstream_oauth_runtime(UpstreamOAuthRuntime::new(
        Arc::new(FakeOAuthProvider),
        managers,
    ));

    let temp = tempfile::tempdir().unwrap();
    let state = crate::testing::oauth_state_with_gateway_product_state(temp.path(), gateway).await;
    let admin_token = protected_route_token(
        &state,
        "https://example.example.com/mcp",
        soma_domain::scopes::ADMIN_SCOPE,
    );
    let route_token = protected_route_token(&state, "https://mcp.example.com/media", "soma:read");
    let app = router(state);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/gateway/gateway.oauth.status")
                .header(header::AUTHORIZATION, format!("Bearer {admin_token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"upstream":"backend"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/media")
                .header(header::HOST, "mcp.example.com")
                .header(header::AUTHORIZATION, format!("Bearer {route_token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"jsonrpc":"2.0","id":1}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(seen_auth.lock().await.as_slice(), ["Bearer oauth-token"]);
}

#[cfg(feature = "oauth")]
fn upstream_oauth_gateway_config() -> GatewayConfig {
    let mut config =
        protected_gateway_config(Some("https://upstream.example/mcp".to_owned()), None);
    config.upstream[0].oauth = Some(soma_gateway::config::GatewayUpstreamOauthConfig {
        mode: soma_gateway::config::GatewayUpstreamOauthMode::AuthorizationCodePkce,
        registration: soma_gateway::config::GatewayUpstreamOauthRegistration::Auto,
        scopes: Some(vec!["mcp".to_owned()]),
        prefer_client_metadata_document: None,
    });
    config
}

#[cfg(feature = "oauth")]
async fn save_callback_state(state: &AppState, csrf: &str, subject: &str) {
    let AuthPolicy::Mounted {
        auth_state: Some(auth_state),
    } = &state.auth_policy
    else {
        panic!("OAuth test state must mount auth state");
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    auth_state
        .store
        .save_upstream_oauth_state(soma_auth::types::UpstreamOauthStateRow {
            upstream_name: "backend".to_owned(),
            subject: subject.to_owned(),
            csrf_token: csrf.to_owned(),
            pkce_verifier: "verifier".to_owned(),
            expected_issuer: Some("https://issuer.example".to_owned()),
            require_issuer: true,
            requested_scopes_json: r#"["mcp"]"#.to_owned(),
            created_at: now,
            expires_at: now + 300,
        })
        .await
        .expect("save callback state");
}

#[cfg(feature = "oauth")]
#[tokio::test]
async fn generated_upstream_client_metadata_is_public_and_web_typed() {
    let temp = tempfile::tempdir().unwrap();
    let state = oauth_state_with_gateway(&temp, upstream_oauth_gateway_config()).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/auth/upstream/client-metadata/backend")
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
    assert_eq!(
        json["client_id"],
        "https://example.example.com/auth/upstream/client-metadata/backend"
    );
    assert_eq!(json["application_type"], "web");
    assert_eq!(
        json["redirect_uris"],
        serde_json::json!(["https://example.example.com/auth/upstream/callback"])
    );
}

#[cfg(feature = "oauth")]
#[tokio::test]
async fn generated_upstream_client_metadata_requires_an_https_public_origin() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = oauth_state_with_gateway(&temp, upstream_oauth_gateway_config()).await;
    state.config.auth.public_url = Some("http://127.0.0.1:40060".to_owned());

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/auth/upstream/client-metadata/backend")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[cfg(feature = "oauth")]
#[tokio::test]
async fn upstream_oauth_callback_rejects_unknown_state() {
    let temp = tempfile::tempdir().unwrap();
    let state = oauth_state_with_gateway(&temp, upstream_oauth_gateway_config()).await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/auth/upstream/callback?code=code&state=unknown&iss=https%3A%2F%2Fissuer.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[cfg(feature = "oauth")]
#[tokio::test]
async fn upstream_oauth_provider_error_consumes_state_without_reflecting_description() {
    let temp = tempfile::tempdir().unwrap();
    let state = oauth_state_with_gateway(&temp, upstream_oauth_gateway_config()).await;
    save_callback_state(&state, "denied-state", "alice").await;

    let response = router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/auth/upstream/callback?state=denied-state&error=access_denied&error_description=super-secret-detail")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(!String::from_utf8_lossy(&body).contains("super-secret-detail"));
    let AuthPolicy::Mounted {
        auth_state: Some(auth_state),
    } = &state.auth_policy
    else {
        panic!("OAuth test state must mount auth state");
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    assert!(auth_state
        .store
        .find_upstream_oauth_state_owner("denied-state", now)
        .await
        .unwrap()
        .is_none());
}

#[cfg(feature = "oauth")]
#[tokio::test]
async fn upstream_oauth_callback_forwards_code_state_and_rfc9207_issuer() {
    let callbacks = Arc::new(Mutex::new(Vec::new()));
    let gateway =
        soma_runtime::server::gateway_product_state_from_config(upstream_oauth_gateway_config())
            .unwrap();
    let mut managers: BTreeMap<String, Arc<dyn UpstreamOAuthManager>> = BTreeMap::new();
    managers.insert(
        "backend".to_owned(),
        Arc::new(RecordingOAuthManager {
            callbacks: Arc::clone(&callbacks),
        }),
    );
    gateway.install_upstream_oauth_runtime(UpstreamOAuthRuntime::new(
        Arc::new(FakeOAuthProvider),
        managers,
    ));
    let temp = tempfile::tempdir().unwrap();
    let state = crate::testing::oauth_state_with_gateway_product_state(temp.path(), gateway).await;
    save_callback_state(&state, "callback-state", "alice").await;

    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/auth/upstream/callback?code=auth-code&state=callback-state&iss=https%3A%2F%2Fissuer.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        callbacks.lock().await.as_slice(),
        [RecordedAuthorizationCallback {
            subject: "alice".to_owned(),
            code: "auth-code".to_owned(),
            state: "callback-state".to_owned(),
            issuer: Some("https://issuer.example".to_owned()),
        }]
    );
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

async fn preflight_allow_headers(state: AppState, requested_headers: &str) -> String {
    let response = router(state)
        .oneshot(
            Request::builder()
                .method(axum::http::Method::OPTIONS)
                .uri("/mcp")
                .header(axum::http::header::ORIGIN, "http://127.0.0.1:40060")
                .header(axum::http::header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                .header(
                    axum::http::header::ACCESS_CONTROL_REQUEST_HEADERS,
                    requested_headers,
                )
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    response
        .headers()
        .get(axum::http::header::ACCESS_CONTROL_ALLOW_HEADERS)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

#[tokio::test]
async fn off_mode_denies_trace_header_preflight() {
    let allow_headers =
        preflight_allow_headers(crate::testing::loopback_state(), "TraceParent").await;
    assert!(
        !allow_headers.contains("traceparent"),
        "off mode must not allow traceparent, got: {allow_headers:?}"
    );
}

#[tokio::test]
async fn trusted_mode_allows_traceparent_and_tracestate_but_not_baggage() {
    let state = crate::testing::loopback_state_with_mcp_config(soma_config::McpConfig {
        trace_headers: soma_config::TraceHeaderMode::Trusted,
        ..soma_config::McpConfig::default()
    });
    let allow_headers = preflight_allow_headers(state, "TraceParent, TraceState, Baggage").await;

    assert!(
        allow_headers.contains("traceparent"),
        "got: {allow_headers:?}"
    );
    assert!(
        allow_headers.contains("tracestate"),
        "got: {allow_headers:?}"
    );
    assert!(
        !allow_headers.contains("baggage"),
        "trusted mode must not allow baggage, got: {allow_headers:?}"
    );
}

#[tokio::test]
async fn trusted_with_baggage_mode_allows_all_three_trace_headers() {
    let state = crate::testing::loopback_state_with_mcp_config(soma_config::McpConfig {
        trace_headers: soma_config::TraceHeaderMode::TrustedWithBaggage,
        ..soma_config::McpConfig::default()
    });
    let allow_headers = preflight_allow_headers(state, "TraceParent, TraceState, Baggage").await;

    for required in ["traceparent", "tracestate", "baggage"] {
        assert!(
            allow_headers.contains(required),
            "CORS allow-headers must include {required}, got: {allow_headers:?}"
        );
    }
}

#[tokio::test]
async fn unmatched_route_returns_the_not_found_envelope() {
    // Regression guard for the fallback swap from an inline
    // `Json(json!({"error": "not_found"}))` closure to
    // `soma_http_server::rejection::not_found_handler`: the composed router
    // must still answer an unmatched path with the same 404 JSON shape --
    // but only when no embedded web assets are present to claim the SPA
    // fallback instead (see `http.rs`'s `router()`: `soma_web::serve_web_assets`
    // intentionally returns 200 with `index.html` for client-side routing
    // when `soma_web::web_assets_available()` is true). This is genuinely
    // build-machine-dependent: `apps/web/out/` is embedded via `include_dir!`
    // at compile time, so a dev box with a prior `apps/web` build present
    // will legitimately take the SPA branch here.
    let response = router(crate::testing::loopback_state())
        .oneshot(
            Request::builder()
                .uri("/this-route-does-not-exist")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");

    #[cfg(feature = "web")]
    fn web_assets_available() -> bool {
        soma_web::web_assets_available()
    }
    #[cfg(not(feature = "web"))]
    fn web_assets_available() -> bool {
        false
    }

    if web_assets_available() {
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should read");
        assert!(
            !bytes.is_empty(),
            "SPA fallback should serve index.html content"
        );
    } else {
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should read");
        let body: serde_json::Value = serde_json::from_slice(&bytes).expect("body should be json");
        assert_eq!(body["error"], "not_found");
    }
}

async fn oauth_state_with_gateway(temp: &tempfile::TempDir, gateway: GatewayConfig) -> AppState {
    crate::testing::oauth_state_with_gateway(temp.path(), gateway).await
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
