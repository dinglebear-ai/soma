use axum::Router;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{Request, StatusCode, header};
use base64::Engine;
use std::net::SocketAddr;
use tower::util::ServiceExt;

use crate::authorize::tests::{
    test_auth_config, test_auth_state_with_config, test_auth_state_with_registered_client,
};
use crate::config::MachineClientConfig;
use crate::state::AuthState;
use crate::types::{RefreshTokenRow, RegisteredClient};
use crate::util::now_unix;

// `oneshot` bypasses the live `into_make_service_with_connect_info` layer, so
// `/revoke`'s rate-limit `ConnectInfo<SocketAddr>` extractor would be missing
// and every request would 500. Wrap the real router with a mock peer address,
// matching the helpers in `authorize.rs` and `token.rs`.
fn router(state: AuthState) -> Router {
    crate::routes::router(state).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 9003))))
}

async fn post_revoke(
    state: &AuthState,
    body: &str,
    authorization: Option<&str>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/revoke")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(authorization) = authorization {
        builder = builder.header(header::AUTHORIZATION, authorization);
    }
    let response = router(state.clone())
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, bytes.to_vec())
}

fn error_code(body: &[u8]) -> String {
    let json: serde_json::Value = serde_json::from_slice(body).unwrap_or(serde_json::Value::Null);
    json["error"].as_str().unwrap_or_default().to_string()
}

async fn register_public_client(state: &AuthState, client_id: &str) {
    state
        .store
        .register_client(RegisteredClient {
            client_id: client_id.to_string(),
            redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
            created_at: now_unix(),
            token_endpoint_auth_method: "none".to_string(),
            jwks: None,
        })
        .await
        .unwrap();
}

async fn seed_refresh_token(state: &AuthState, refresh_token: &str, client_id: &str) {
    state
        .store
        .upsert_refresh_token(RefreshTokenRow {
            refresh_token: refresh_token.to_string(),
            client_id: client_id.to_string(),
            subject: "google-subject-123".to_string(),
            resource: "https://app.example.com/mcp".to_string(),
            scope: "app:read".to_string(),
            provider: "google".to_string(),
            provider_refresh_token: None,
            created_at: now_unix(),
            expires_at: now_unix() + 3600,
            token_endpoint_auth_method: None,
        })
        .await
        .unwrap();
}

async fn refresh_token_exists(state: &AuthState, refresh_token: &str) -> bool {
    state
        .store
        .find_refresh_token(refresh_token)
        .await
        .unwrap()
        .is_some()
}

#[tokio::test]
async fn revoking_a_refresh_token_succeeds_and_the_token_stops_working() {
    let state = test_auth_state_with_registered_client().await;
    seed_refresh_token(&state, "live-refresh", "client").await;
    assert!(refresh_token_exists(&state, "live-refresh").await);

    let (status, body) = post_revoke(&state, "token=live-refresh&client_id=client", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.is_empty(),
        "RFC 7009 section 2.2 revocation responses carry no body: {}",
        String::from_utf8_lossy(&body)
    );
    assert!(!refresh_token_exists(&state, "live-refresh").await);

    // End to end, not just at the store: the revoked token must no longer
    // redeem at `/token`.
    let response = router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/token")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(
                    "grant_type=refresh_token&client_id=client&refresh_token=live-refresh",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(error_code(&body), "invalid_grant");
}

#[tokio::test]
async fn revoking_an_unknown_token_returns_200() {
    // RFC 7009 section 2.2: an invalid token is not an error, precisely so
    // `/revoke` cannot be used as a token-existence oracle. The response must
    // be byte-for-byte what a real revocation returns.
    let state = test_auth_state_with_registered_client().await;
    seed_refresh_token(&state, "live-refresh", "client").await;

    let (real_status, real_body) =
        post_revoke(&state, "token=live-refresh&client_id=client", None).await;
    let (unknown_status, unknown_body) =
        post_revoke(&state, "token=never-issued-at-all&client_id=client", None).await;

    assert_eq!(unknown_status, StatusCode::OK);
    assert_eq!(unknown_status, real_status);
    assert_eq!(unknown_body, real_body);
}

#[tokio::test]
async fn a_client_cannot_revoke_another_clients_refresh_token() {
    let state = test_auth_state_with_registered_client().await;
    register_public_client(&state, "other-client").await;
    seed_refresh_token(&state, "other-clients-refresh", "other-client").await;

    let (status, body) =
        post_revoke(&state, "token=other-clients-refresh&client_id=client", None).await;

    // 200 and an empty body, exactly like an unknown token -- the caller must
    // not learn that this token exists and belongs to somebody else.
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_empty());
    assert!(
        refresh_token_exists(&state, "other-clients-refresh").await,
        "a client must never be able to revoke another client's refresh token"
    );
}

#[tokio::test]
async fn revoke_is_rate_limited() {
    let mut config = test_auth_config();
    config.token_requests_per_minute = 1;
    let state = test_auth_state_with_config(config).await;
    register_public_client(&state, "client").await;

    let (first, _) = post_revoke(&state, "token=anything&client_id=client", None).await;
    assert_eq!(first, StatusCode::OK);

    let (second, body) = post_revoke(&state, "token=anything&client_id=client", None).await;
    assert_eq!(second, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(error_code(&body), "temporarily_unavailable");
}

#[tokio::test]
async fn revoke_rate_limits_before_looking_anything_up() {
    // The limiter must fire even for a request that would otherwise be
    // rejected at client authentication, proving the guard runs ahead of any
    // store read or CIMD metadata fetch.
    let mut config = test_auth_config();
    config.token_requests_per_minute = 1;
    let state = test_auth_state_with_config(config).await;

    let (first, _) = post_revoke(&state, "token=anything&client_id=unregistered", None).await;
    assert_eq!(first, StatusCode::UNAUTHORIZED);

    let (second, body) = post_revoke(&state, "token=anything&client_id=unregistered", None).await;
    assert_eq!(second, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(error_code(&body), "temporarily_unavailable");
}

#[tokio::test]
async fn revoke_rejects_an_unknown_client() {
    let state = test_auth_state_with_registered_client().await;
    seed_refresh_token(&state, "live-refresh", "client").await;

    let (status, body) = post_revoke(&state, "token=live-refresh&client_id=nobody", None).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(&body), "invalid_client");
    assert!(refresh_token_exists(&state, "live-refresh").await);
}

#[tokio::test]
async fn revoke_requires_a_client_id() {
    let state = test_auth_state_with_registered_client().await;

    let (status, body) = post_revoke(&state, "token=live-refresh", None).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error_code(&body), "invalid_request");
}

#[tokio::test]
async fn revoke_reports_unsupported_token_type_for_an_access_token_hint() {
    // Access tokens are stateless JWTs with no server-side record, so RFC 7009
    // section 2.2.1's `unsupported_token_type` is the honest answer rather
    // than a 200 that claims a revocation which never happened.
    let state = test_auth_state_with_registered_client().await;

    let (status, body) = post_revoke(
        &state,
        "token=some.jwt.value&client_id=client&token_type_hint=access_token",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error_code(&body), "unsupported_token_type");
}

#[tokio::test]
async fn revoke_ignores_a_bogus_token_type_hint() {
    // RFC 7009 section 2.2: "An invalid token type hint value is ignored by
    // the authorization server and does not influence the revocation
    // response."
    let state = test_auth_state_with_registered_client().await;
    seed_refresh_token(&state, "live-refresh", "client").await;

    let (status, _) = post_revoke(
        &state,
        "token=live-refresh&client_id=client&token_type_hint=not-a-real-hint",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(!refresh_token_exists(&state, "live-refresh").await);
}

fn basic_authorization(client_id: &str, client_secret: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{client_id}:{client_secret}"))
    )
}

async fn secret_machine_client_state() -> AuthState {
    let mut config = test_auth_config();
    config.machine_clients = vec![MachineClientConfig {
        client_id: "machine".to_string(),
        client_secret: Some("machine-secret".to_string()),
        jwks: None,
        scopes: vec!["app:read".to_string()],
        resources: vec!["https://app.example.com/mcp".to_string()],
    }];
    test_auth_state_with_config(config).await
}

#[tokio::test]
async fn revoke_accepts_client_secret_basic_for_a_confidential_client() {
    let state = secret_machine_client_state().await;
    seed_refresh_token(&state, "machine-refresh", "machine").await;

    let (status, body) = post_revoke(
        &state,
        "token=machine-refresh",
        Some(&basic_authorization("machine", "machine-secret")),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    assert!(!refresh_token_exists(&state, "machine-refresh").await);
}

#[tokio::test]
async fn revoke_rejects_a_wrong_client_secret() {
    let state = secret_machine_client_state().await;
    seed_refresh_token(&state, "machine-refresh", "machine").await;

    let (status, body) = post_revoke(
        &state,
        "token=machine-refresh",
        Some(&basic_authorization("machine", "not-the-secret")),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(error_code(&body), "invalid_client");
    assert!(refresh_token_exists(&state, "machine-refresh").await);
}
