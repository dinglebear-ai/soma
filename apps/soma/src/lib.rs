//! `soma` library crate.
//!
//! Exposes the service layer, config, and transport client so that integration
//! tests can import them without duplicating state construction.
//!
//! Public modules:
//!   [`app`]     — `SomaService` (business logic)
//!   [`config`]  — `Config`, `SomaConfig`, `McpConfig`
//!   [`soma`]    — `SomaClient` (transport stub)
//!   [`mcp`]     — MCP protocol layer (enabled by `mcp`)
//!   [`server`]  — `AppState`, `AuthPolicy`, HTTP router (enabled by `cli`, `mcp`, or `api`)
//!   [`api`]     — REST API handlers (enabled by `api`)

#[cfg(feature = "api")]
pub use soma_api::api;
#[cfg(feature = "api")]
pub use soma_api::gateway as gateway_api;
#[cfg(feature = "cli")]
pub use soma_cli as cli;
pub use soma_contracts::actions;
pub use soma_contracts::config;
pub use soma_contracts::env_registry;
#[cfg(feature = "mcp")]
pub use soma_mcp as mcp;
#[cfg(feature = "observability")]
pub use soma_observability::binary_status;
#[cfg(feature = "observability")]
pub use soma_observability::logging;
pub use soma_service::app;
pub use soma_service::soma;
#[cfg(any(feature = "cli", feature = "mcp-stdio", feature = "mcp-http"))]
pub mod runtime;
pub use soma_contracts::token_limit;
#[cfg(feature = "web")]
pub use soma_web as web;

#[cfg(feature = "oauth")]
mod gateway_auth;
#[cfg(feature = "mcp-http")]
mod protected_routes;
#[cfg(feature = "mcp-http")]
mod protected_routes_proxy;
#[cfg(feature = "mcp-http")]
mod routes;

#[cfg(any(feature = "cli", feature = "mcp", feature = "api"))]
pub mod server {
    pub use soma_runtime::server::*;

    #[cfg(feature = "mcp-http")]
    pub use crate::routes::router;
}

/// Test helpers — available when `features = ["test-support"]` or in `cfg(test)`.
///
/// Use these in integration tests to construct `AppState` without real creds.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
#[cfg(any(feature = "cli", feature = "mcp", feature = "api"))]
pub mod testing {
    #[cfg(feature = "auth")]
    use std::sync::Arc;

    use crate::{
        app::SomaService,
        config::{McpConfig, SomaConfig},
        server::{AppState, AuthPolicy},
        soma::SomaClient,
    };
    use soma_runtime::server::empty_gateway_product_state;

    fn stub_service() -> SomaService {
        let client = SomaClient::new(&SomaConfig {
            api_url: String::new(),
            api_key: "test".into(),
            ..SomaConfig::default()
        })
        .expect("stub client should always build");
        SomaService::new(client)
    }

    /// `AppState` with no auth (loopback trust boundary).
    /// Use this for unit tests that don't need auth.
    pub fn loopback_state() -> AppState {
        let service = stub_service();
        let provider_registry =
            soma_service::static_provider_registry(service.clone()).expect("static registry");
        AppState {
            config: McpConfig::default(),
            auth_policy: AuthPolicy::LoopbackDev,
            service,
            provider_registry,
            gateway: empty_gateway_product_state(),
            remote_adapter: false,
            response_pages: Default::default(),
        }
    }

    /// `AppState` requiring a static bearer token.
    pub fn bearer_state(token: &str) -> AppState {
        let service = stub_service();
        let provider_registry =
            soma_service::static_provider_registry(service.clone()).expect("static registry");
        AppState {
            config: McpConfig {
                api_token: Some(token.to_string()),
                ..McpConfig::default()
            },
            auth_policy: mounted_test_policy(),
            service,
            provider_registry,
            gateway: empty_gateway_product_state(),
            remote_adapter: false,
            response_pages: Default::default(),
        }
    }

    /// `AppState` with full OAuth (requires data directory for SQLite + key file).
    #[cfg(feature = "auth")]
    pub async fn oauth_state(data_dir: &std::path::Path) -> AppState {
        let auth_state = build_auth_state(data_dir).await;
        let service = stub_service();
        let provider_registry =
            soma_service::static_provider_registry(service.clone()).expect("static registry");
        AppState {
            config: McpConfig {
                auth: soma_contracts::config::AuthConfig {
                    public_url: Some("https://example.example.com".to_string()),
                    ..Default::default()
                },
                ..McpConfig::default()
            },
            auth_policy: AuthPolicy::Mounted {
                auth_state: Some(Arc::new(auth_state)),
            },
            service,
            provider_registry,
            gateway: empty_gateway_product_state(),
            remote_adapter: false,
            response_pages: Default::default(),
        }
    }

    #[cfg(feature = "auth")]
    pub async fn build_auth_state(data_dir: &std::path::Path) -> soma_auth::state::AuthState {
        let vars: Vec<(String, String)> = vec![
            ("SOMA_MCP_AUTH_MODE".into(), "oauth".into()),
            (
                "SOMA_MCP_PUBLIC_URL".into(),
                "https://example.example.com".into(),
            ),
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

        let auth_config = soma_auth::config::AuthConfigBuilder::new()
            .env_prefix("SOMA_MCP")
            .session_cookie_name("soma_mcp_session")
            .scopes_supported(vec![
                soma_contracts::actions::READ_SCOPE.into(),
                soma_contracts::actions::WRITE_SCOPE.into(),
                soma_contracts::scopes::ADMIN_SCOPE.into(),
            ])
            .default_scope("soma:read")
            .resource_path("/mcp")
            .build_from_sources(vars)
            .expect("test auth config should build");

        soma_auth::state::AuthState::new(auth_config)
            .await
            .expect("test auth state should init")
    }

    #[cfg(feature = "auth")]
    fn mounted_test_policy() -> AuthPolicy {
        AuthPolicy::Mounted { auth_state: None }
    }

    #[cfg(not(feature = "auth"))]
    fn mounted_test_policy() -> AuthPolicy {
        AuthPolicy::Mounted {}
    }
}
