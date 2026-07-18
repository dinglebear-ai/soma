use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
    middleware, Router,
};
use soma_gateway::config::ProtectedMcpRouteConfig;
use tower::ServiceExt;

use crate::test_support;

use super::{
    is_reserved_public_path, is_route_metadata_path, route_metadata_url, ProtectedMcpState,
};

fn route() -> ProtectedMcpRouteConfig {
    ProtectedMcpRouteConfig {
        name: "media".to_owned(),
        public_host: "MCP.Example.COM.".to_owned(),
        public_path: "/media".to_owned(),
        scopes: vec!["soma:read".to_owned()],
        ..ProtectedMcpRouteConfig::default()
    }
}

#[test]
fn metadata_url_uses_normalized_host_and_route_path() {
    assert_eq!(
        route_metadata_url(&route()),
        "https://mcp.example.com/.well-known/oauth-protected-resource/media"
    );
}

#[test]
fn route_metadata_path_is_route_relative() {
    assert!(is_route_metadata_path(
        &route(),
        "/media/.well-known/oauth-protected-resource"
    ));
    assert!(!is_route_metadata_path(
        &route(),
        "/.well-known/oauth-protected-resource/media"
    ));
}

#[test]
fn oauth_public_paths_bypass_protected_route_intercept() {
    assert!(is_reserved_public_path(
        "/.well-known/oauth-protected-resource/media"
    ));
    assert!(is_reserved_public_path("/authorize"));
    assert!(!is_reserved_public_path("/media"));
}

// --- `protected_mcp_intercept` axum-harness tests ---
//
// These drive the real middleware (not just its helper functions) with a
// real signed access token and a real `AppState`/`AuthState`, matching the
// no-token/malformed-token/insufficient-scope/admin-bypass cases the PR 18
// review flagged as untested for this security-critical path. The route has
// no `backend_url`/`upstream`/`target`, so once auth succeeds, dispatch
// falls through to `proxy_protected_mcp_route` and fails with a `502
// missing_target` — that failure (rather than a 401/403) is exactly how
// these tests observe "authentication/authorization passed".

async fn app(state: ProtectedMcpState) -> Router {
    Router::new().layer(middleware::from_fn_with_state(
        state,
        super::protected_mcp_intercept,
    ))
}

fn get_media_request(bearer: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(Method::GET)
        .uri("/media")
        .header(header::HOST, "mcp.example.com");
    if let Some(token) = bearer {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    builder.body(Body::empty()).unwrap()
}

async fn error_code(response: axum::response::Response) -> String {
    let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    json["error"].as_str().unwrap_or_default().to_owned()
}

#[tokio::test]
async fn missing_bearer_token_is_rejected_with_401() {
    let data_dir = tempfile::tempdir().unwrap();
    let auth_state = test_support::auth_state(data_dir.path()).await;
    let gateway = test_support::gateway_with_routes(vec![test_support::route()]);
    let runtime_state = test_support::mounted_app_state(gateway, Some(auth_state));
    let mcp_state = test_support::mcp_state(&runtime_state);
    let state = ProtectedMcpState::new(runtime_state, mcp_state);

    let response = app(state)
        .await
        .oneshot(get_media_request(None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(response.headers().contains_key(header::WWW_AUTHENTICATE));
    assert_eq!(error_code(response).await, "unauthorized");
}

#[tokio::test]
async fn malformed_bearer_token_is_rejected_with_401() {
    let data_dir = tempfile::tempdir().unwrap();
    let auth_state = test_support::auth_state(data_dir.path()).await;
    let gateway = test_support::gateway_with_routes(vec![test_support::route()]);
    let runtime_state = test_support::mounted_app_state(gateway, Some(auth_state));
    let mcp_state = test_support::mcp_state(&runtime_state);
    let state = ProtectedMcpState::new(runtime_state, mcp_state);

    let response = app(state)
        .await
        .oneshot(get_media_request(Some("not-a-real-jwt")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(response).await, "unauthorized");
}

#[tokio::test]
async fn insufficient_scope_is_rejected_with_403() {
    let data_dir = tempfile::tempdir().unwrap();
    let auth_state = test_support::auth_state(data_dir.path()).await;
    let gateway = test_support::gateway_with_routes(vec![test_support::route()]);
    // Token is validly signed for the right issuer/audience, but carries a
    // scope the route does not require and lacks the route's required
    // `soma:read` scope.
    let token = test_support::issue_token(&auth_state, &test_support::route(), "soma:write");
    let runtime_state = test_support::mounted_app_state(gateway, Some(auth_state));
    let mcp_state = test_support::mcp_state(&runtime_state);
    let state = ProtectedMcpState::new(runtime_state, mcp_state);

    let response = app(state)
        .await
        .oneshot(get_media_request(Some(&token)))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(error_code(response).await, "insufficient_scope");
}

#[tokio::test]
async fn admin_scope_bypasses_the_route_scope_requirement() {
    let data_dir = tempfile::tempdir().unwrap();
    let auth_state = test_support::auth_state(data_dir.path()).await;
    let gateway = test_support::gateway_with_routes(vec![test_support::route()]);
    // Admin scope only — does not include the route's required `soma:read`.
    let token = test_support::issue_token(
        &auth_state,
        &test_support::route(),
        soma_domain::scopes::ADMIN_SCOPE,
    );
    let runtime_state = test_support::mounted_app_state(gateway, Some(auth_state));
    let mcp_state = test_support::mcp_state(&runtime_state);
    let state = ProtectedMcpState::new(runtime_state, mcp_state);

    let response = app(state)
        .await
        .oneshot(get_media_request(Some(&token)))
        .await
        .unwrap();

    // Authentication/authorization succeeded (not 401/403); dispatch then
    // fails downstream because the test route has no backend_url/upstream —
    // that 502 is this test's proof that auth was not the reason it failed.
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
    assert_ne!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(error_code(response).await, "missing_target");
}

#[tokio::test]
async fn sufficient_scope_passes_authentication() {
    let data_dir = tempfile::tempdir().unwrap();
    let auth_state = test_support::auth_state(data_dir.path()).await;
    let gateway = test_support::gateway_with_routes(vec![test_support::route()]);
    let token = test_support::issue_token(&auth_state, &test_support::route(), "soma:read");
    let runtime_state = test_support::mounted_app_state(gateway, Some(auth_state));
    let mcp_state = test_support::mcp_state(&runtime_state);
    let state = ProtectedMcpState::new(runtime_state, mcp_state);

    let response = app(state)
        .await
        .oneshot(get_media_request(Some(&token)))
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
    assert_ne!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(error_code(response).await, "missing_target");
}

#[tokio::test]
async fn missing_oauth_auth_state_is_rejected_with_401() {
    // `AuthPolicy::Mounted { auth_state: None }` (bearer-only mode, no OAuth
    // configured) must fail closed on protected routes rather than silently
    // allowing the request through.
    let gateway = test_support::gateway_with_routes(vec![test_support::route()]);
    let runtime_state = test_support::mounted_app_state(gateway, None);
    let mcp_state = test_support::mcp_state(&runtime_state);
    let state = ProtectedMcpState::new(runtime_state, mcp_state);

    let response = app(state)
        .await
        .oneshot(get_media_request(Some("irrelevant")))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(response).await, "unauthorized");
}

#[tokio::test]
async fn unmatched_route_bypasses_the_middleware_entirely() {
    let data_dir = tempfile::tempdir().unwrap();
    let auth_state = test_support::auth_state(data_dir.path()).await;
    let gateway = test_support::gateway_with_routes(vec![test_support::route()]);
    let runtime_state = test_support::mounted_app_state(gateway, Some(auth_state));
    let mcp_state = test_support::mcp_state(&runtime_state);
    let state = ProtectedMcpState::new(runtime_state, mcp_state);

    let request = Request::builder()
        .method(Method::GET)
        .uri("/unrelated")
        .header(header::HOST, "mcp.example.com")
        .body(Body::empty())
        .unwrap();
    let response = app(state).await.oneshot(request).await.unwrap();

    // No route matches `/unrelated`, so the middleware calls `next.run()`
    // and this hits axum's default "no route registered" 404 — not the
    // protected-route 401/403 path.
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
