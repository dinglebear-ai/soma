//! `rmcp-template` library crate.
//!
//! Exposes the service layer, config, and transport client so that integration
//! tests can import them without duplicating state construction.
//!
//! Public modules:
//!   [`app`]     — `ExampleService` (business logic)
//!   [`config`]  — `Config`, `ExampleConfig`, `McpConfig`
//!   [`example`] — `ExampleClient` (transport stub)
//!   [`mcp`]     — MCP protocol layer (enabled by `mcp`)
//!   [`server`]  — `AppState`, `AuthPolicy`, HTTP router (enabled by `cli`, `mcp`, or `api`)
//!   [`api`]     — REST API handlers (enabled by `api`)

#[cfg(feature = "api")]
pub use rtemplate_api::api;
#[cfg(feature = "cli")]
pub use rtemplate_cli as cli;
pub use rtemplate_contracts::actions;
pub use rtemplate_contracts::config;
pub use rtemplate_contracts::env_registry;
#[cfg(feature = "mcp")]
pub use rtemplate_mcp as mcp;
#[cfg(feature = "observability")]
pub use rtemplate_observability::binary_status;
#[cfg(feature = "observability")]
pub use rtemplate_observability::logging;
pub use rtemplate_service::app;
pub use rtemplate_service::example;
#[cfg(any(feature = "cli", feature = "mcp-stdio", feature = "mcp-http"))]
pub mod runtime;
pub use rtemplate_contracts::token_limit;
#[cfg(feature = "web")]
pub use rtemplate_web as web;

#[cfg(feature = "mcp-http")]
mod routes;

#[cfg(any(feature = "cli", feature = "mcp", feature = "api"))]
pub mod server {
    pub use rtemplate_runtime::server::*;

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
        app::ExampleService,
        config::{ExampleConfig, McpConfig},
        example::ExampleClient,
        server::{AppState, AuthPolicy},
    };

    fn stub_service() -> ExampleService {
        let client = ExampleClient::new(&ExampleConfig {
            api_url: String::new(),
            api_key: "test".into(),
        })
        .expect("stub client should always build");
        ExampleService::new(client)
    }

    /// `AppState` with no auth (loopback trust boundary).
    /// Use this for unit tests that don't need auth.
    pub fn loopback_state() -> AppState {
        let service = stub_service();
        let provider_registry =
            rtemplate_service::static_provider_registry(service.clone()).expect("static registry");
        AppState {
            config: McpConfig::default(),
            auth_policy: AuthPolicy::LoopbackDev,
            service,
            provider_registry,
            response_pages: Default::default(),
        }
    }

    /// `AppState` requiring a static bearer token.
    pub fn bearer_state(token: &str) -> AppState {
        let service = stub_service();
        let provider_registry =
            rtemplate_service::static_provider_registry(service.clone()).expect("static registry");
        AppState {
            config: McpConfig {
                api_token: Some(token.to_string()),
                ..McpConfig::default()
            },
            auth_policy: mounted_test_policy(),
            service,
            provider_registry,
            response_pages: Default::default(),
        }
    }

    /// `AppState` with full OAuth (requires data directory for SQLite + key file).
    #[cfg(feature = "auth")]
    pub async fn oauth_state(data_dir: &std::path::Path) -> AppState {
        let auth_state = build_auth_state(data_dir).await;
        let service = stub_service();
        let provider_registry =
            rtemplate_service::static_provider_registry(service.clone()).expect("static registry");
        AppState {
            config: McpConfig {
                auth: rtemplate_contracts::config::AuthConfig {
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
            response_pages: Default::default(),
        }
    }

    #[cfg(feature = "auth")]
    pub async fn build_auth_state(data_dir: &std::path::Path) -> rtemplate_auth::state::AuthState {
        let vars: Vec<(String, String)> = vec![
            ("RTEMPLATE_MCP_AUTH_MODE".into(), "oauth".into()),
            (
                "RTEMPLATE_MCP_PUBLIC_URL".into(),
                "https://example.example.com".into(),
            ),
            (
                "RTEMPLATE_MCP_GOOGLE_CLIENT_ID".into(),
                "test-client-id".into(),
            ),
            (
                "RTEMPLATE_MCP_GOOGLE_CLIENT_SECRET".into(),
                "test-client-secret".into(),
            ),
            (
                "RTEMPLATE_MCP_AUTH_ADMIN_EMAIL".into(),
                "admin@example.com".into(),
            ),
            (
                "RTEMPLATE_MCP_AUTH_SQLITE_PATH".into(),
                data_dir.join("auth.db").display().to_string(),
            ),
            (
                "RTEMPLATE_MCP_AUTH_KEY_PATH".into(),
                data_dir.join("auth-jwt.pem").display().to_string(),
            ),
        ];

        let auth_config = rtemplate_auth::config::AuthConfigBuilder::new()
            .env_prefix("RTEMPLATE_MCP")
            .session_cookie_name("example_mcp_session")
            .scopes_supported(vec![
                rtemplate_contracts::actions::READ_SCOPE.into(),
                rtemplate_contracts::actions::WRITE_SCOPE.into(),
            ])
            .default_scope("example:read")
            .resource_path("/mcp")
            .build_from_sources(vars)
            .expect("test auth config should build");

        rtemplate_auth::state::AuthState::new(auth_config)
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
