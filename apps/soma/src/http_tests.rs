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
use futures::future::BoxFuture;
#[cfg(feature = "oauth")]
use mcp_client::{
    oauth::{
        BeginAuthorization, UpstreamOAuthCredentialStatus, UpstreamOAuthError,
        UpstreamOAuthHttpClient, UpstreamOAuthManager, UpstreamOAuthProvider, UpstreamOAuthRuntime,
    },
    upstream::http_body_cap::BodyCappedHttpClient,
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
struct FakeOAuthProvider;

#[cfg(feature = "oauth")]
impl UpstreamOAuthProvider for FakeOAuthProvider {
    fn authenticated_http_client<'a>(
        &'a self,
        _upstream: &'a mcp_client::config::UpstreamConfig,
        _subject: &'a str,
        _http_client: BodyCappedHttpClient,
    ) -> BoxFuture<'a, Result<UpstreamOAuthHttpClient, UpstreamOAuthError>> {
        Box::pin(async {
            Err(UpstreamOAuthError::internal(
                "unused by protected proxy test",
            ))
        })
    }
}

#[cfg(feature = "oauth")]
struct FakeOAuthManager;

#[cfg(feature = "oauth")]
impl UpstreamOAuthManager for FakeOAuthManager {
    fn begin_authorization<'a>(
        &'a self,
        _subject: &'a str,
    ) -> BoxFuture<'a, Result<BeginAuthorization, UpstreamOAuthError>> {
        Box::pin(async {
            Err(UpstreamOAuthError::internal(
                "unused by protected proxy test",
            ))
        })
    }

    fn credential_status<'a>(
        &'a self,
        _subject: &'a str,
    ) -> BoxFuture<'a, Result<Option<UpstreamOAuthCredentialStatus>, UpstreamOAuthError>> {
        Box::pin(async {
            Ok(Some(UpstreamOAuthCredentialStatus {
                access_token_expires_at: 4_102_444_800,
                refresh_token_present: true,
            }))
        })
    }

    fn clear_credentials<'a>(
        &'a self,
        _subject: &'a str,
    ) -> BoxFuture<'a, Result<(), UpstreamOAuthError>> {
        Box::pin(async { Ok(()) })
    }

    fn access_token<'a>(
        &'a self,
        _subject: &'a str,
    ) -> BoxFuture<'a, Result<String, UpstreamOAuthError>> {
        Box::pin(async { Ok("oauth-token".to_owned()) })
    }
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
