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

pub mod actions;
#[cfg(feature = "api")]
pub mod api;
pub mod app;
pub mod binary_status;
#[cfg(feature = "cli")]
pub mod cli;
pub mod config;
pub mod env_registry;
pub mod example;
pub mod logging;
#[cfg(feature = "mcp")]
pub mod mcp;
#[cfg(any(feature = "cli", feature = "mcp-stdio", feature = "mcp-http"))]
pub mod runtime;
#[cfg(any(feature = "cli", feature = "mcp", feature = "api"))]
pub mod server;
pub mod token_limit;
#[cfg(feature = "web")]
pub mod web;

/// Test helpers — available when `features = ["test-support"]` or in `cfg(test)`.
///
/// Use these in integration tests to construct `AppState` without real creds.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
#[cfg(any(feature = "cli", feature = "mcp", feature = "api"))]
pub mod testing {
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
        AppState {
            config: McpConfig::default(),
            auth_policy: AuthPolicy::LoopbackDev,
            service: stub_service(),
            response_pages: Default::default(),
        }
    }

    /// `AppState` requiring a static bearer token.
    pub fn bearer_state(token: &str) -> AppState {
        AppState {
            config: McpConfig {
                api_token: Some(token.to_string()),
                ..McpConfig::default()
            },
            auth_policy: AuthPolicy::Mounted { auth_state: None },
            service: stub_service(),
            response_pages: Default::default(),
        }
    }

    /// `AppState` with full OAuth (requires data directory for SQLite + key file).
    pub async fn oauth_state(data_dir: &std::path::Path) -> AppState {
        let auth_state = build_auth_state(data_dir).await;
        AppState {
            config: McpConfig {
                auth: crate::config::AuthConfig {
                    public_url: Some("https://example.example.com".to_string()),
                    ..Default::default()
                },
                ..McpConfig::default()
            },
            auth_policy: AuthPolicy::Mounted {
                auth_state: Some(Arc::new(auth_state)),
            },
            service: stub_service(),
            response_pages: Default::default(),
        }
    }

    pub async fn build_auth_state(data_dir: &std::path::Path) -> lab_auth::state::AuthState {
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

        let auth_config = lab_auth::config::AuthConfigBuilder::new()
            .env_prefix("RTEMPLATE_MCP")
            .session_cookie_name("example_mcp_session")
            .scopes_supported(vec![
                crate::actions::READ_SCOPE.into(),
                crate::actions::WRITE_SCOPE.into(),
            ])
            .default_scope("example:read")
            .resource_path("/mcp")
            .build_from_sources(vars)
            .expect("test auth config should build");

        lab_auth::state::AuthState::new(auth_config)
            .await
            .expect("test auth state should init")
    }
}
