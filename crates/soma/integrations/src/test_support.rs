//! Shared test-only harness for `protected_routes`/`protected_routes_proxy`
//! axum-integration tests: a real signed OAuth access token, a real
//! `AppState` wired to a single protected MCP route, and a real
//! `ProtectedMcpState` for driving `protected_mcp_intercept` end to end.
//!
//! `soma-service`/`soma-client`/`soma-config`/`tempfile` are dev-dependencies
//! only (see `Cargo.toml`) — `cargo xtask check-architecture` only walks
//! `normal`-kind dependency edges, so this does not create a
//! product-integration -> legacy/product-support layer edge for real builds.
#![cfg(test)]

use std::sync::Arc;

use soma_application::{ApplicationPorts, SomaApplication};
use soma_auth::jwt::AccessClaims;
use soma_auth::state::AuthState;
use soma_client::SomaClient;
use soma_config::{McpConfig, SomaConfig};
use soma_gateway::config::{GatewayConfig, ProtectedMcpRouteConfig};
use soma_mcp::McpState;
use soma_runtime::server::{
    gateway_product_state_from_config, AppState, AuthPolicy, GatewayProductState, SomaRuntime,
};
use soma_service::SomaService;

/// Issuer used by every test token/route pair — matches `route().public_host`.
pub(crate) const ISSUER: &str = "https://mcp.example.com";

/// A single enabled protected route: `soma:read` scope, no gateway subset
/// target (so tests exercise the proxy path), no `backend_url`/`upstream`
/// (so proxy-target-resolution tests can set those per case).
pub(crate) fn route() -> ProtectedMcpRouteConfig {
    ProtectedMcpRouteConfig {
        name: "media".to_owned(),
        public_host: "mcp.example.com".to_owned(),
        public_path: "/media".to_owned(),
        scopes: vec!["soma:read".to_owned()],
        enabled: true,
        ..ProtectedMcpRouteConfig::default()
    }
}

fn stub_service() -> SomaService {
    let client = SomaClient::new(&SomaConfig {
        api_url: String::new(),
        api_key: "test".into(),
        ..SomaConfig::default()
    })
    .expect("stub client should always build");
    SomaService::new(client)
}

pub(crate) fn gateway_with_routes(routes: Vec<ProtectedMcpRouteConfig>) -> GatewayProductState {
    gateway_product_state_from_config(GatewayConfig {
        protected_mcp_routes: routes,
        ..GatewayConfig::default()
    })
    .expect("gateway product state should build")
}

/// A real `AuthState` (real RSA signing keypair, real SQLite-backed
/// resource-scope store) rooted at `data_dir`. `data_dir` must outlive the
/// returned state (e.g. a `tempfile::tempdir()` held by the caller).
pub(crate) async fn auth_state(data_dir: &std::path::Path) -> Arc<AuthState> {
    let vars: Vec<(String, String)> = vec![
        ("SOMA_MCP_AUTH_MODE".into(), "oauth".into()),
        ("SOMA_MCP_PUBLIC_URL".into(), ISSUER.into()),
        ("SOMA_MCP_GOOGLE_CLIENT_ID".into(), "test-client-id".into()),
        (
            "SOMA_MCP_GOOGLE_CLIENT_SECRET".into(),
            "test-client-secret".into(),
        ),
        (
            "SOMA_MCP_AUTH_ADMIN_EMAIL".into(),
            "admin@example.com".into(),
        ),
        (
            "SOMA_MCP_AUTH_SQLITE_PATH".into(),
            data_dir.join("auth.db").display().to_string(),
        ),
        (
            "SOMA_MCP_AUTH_KEY_PATH".into(),
            data_dir.join("auth-jwt.pem").display().to_string(),
        ),
    ];
    let auth_config = crate::auth::soma_auth_config_builder()
        .build_from_sources(vars)
        .expect("test auth config should build");
    Arc::new(
        AuthState::new(auth_config)
            .await
            .expect("test auth state should init"),
    )
}

/// A real `AppState` (`AuthPolicy::Mounted`) wired to `gateway`'s protected
/// routes, using a stub `SomaService`/`ProviderRegistry` (no real upstream —
/// the tests here only exercise auth/routing, never `execute_action`).
pub(crate) fn mounted_app_state(
    gateway: GatewayProductState,
    auth_state: Option<Arc<AuthState>>,
) -> AppState {
    let service = stub_service();
    let provider_registry =
        soma_service::static_provider_registry(service.clone()).expect("static registry");
    let ports = ApplicationPorts::unavailable()
        .with_gateway(Arc::new(crate::GatewayApplicationPort::new(
            gateway.clone(),
        )))
        .with_codemode(Arc::new(crate::CodeModeApplicationPort::default()));
    let application = Arc::new(SomaApplication::new(
        Arc::new(service),
        Arc::new(provider_registry),
        ports,
    ));
    let runtime = Arc::new(SomaRuntime::new(application, gateway));
    AppState::new(
        McpConfig::default(),
        AuthPolicy::Mounted { auth_state },
        runtime,
        Default::default(),
    )
}

pub(crate) fn mcp_state(state: &AppState) -> McpState {
    McpState::new(
        state.application_handle(),
        state.config.clone(),
        soma_domain::AuthorizationMode::Mounted,
        state.response_pages.clone(),
    )
}

/// Sign a valid access token for `route`'s resource with the given `scope`
/// string (space-separated, may be empty).
pub(crate) fn issue_token(
    auth_state: &AuthState,
    route: &ProtectedMcpRouteConfig,
    scope: &str,
) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after the epoch")
        .as_secs() as usize;
    let claims = AccessClaims {
        iss: ISSUER.to_owned(),
        sub: "test-subject".to_owned(),
        aud: route.public_resource(),
        exp: now + 3600,
        iat: now,
        jti: "test-jti".to_owned(),
        scope: scope.to_owned(),
        azp: "test-client".to_owned(),
    };
    auth_state
        .signing_keys
        .issue_access_token(&claims)
        .expect("test token should sign")
}
