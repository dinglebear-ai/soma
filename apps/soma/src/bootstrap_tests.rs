#[cfg(feature = "mcp")]
use soma_application::{CodeModeExecuteRequest, ExecutionContext};
#[cfg(feature = "mcp")]
use soma_client::SomaClient;
#[cfg(feature = "mcp")]
use soma_config::{McpConfig, SomaConfig};
#[cfg(feature = "mcp")]
use soma_domain::{AuthorizationMode, RequestId, Surface};
#[cfg(feature = "mcp")]
use soma_runtime::server::{empty_gateway_product_state, AppState, AuthPolicy};
#[cfg(feature = "mcp")]
use soma_service::SomaService;

#[cfg(feature = "mcp")]
use super::{authorization_mode, runtime_for_components};

#[cfg(feature = "mcp")]
fn state(auth_policy: AuthPolicy) -> AppState {
    let service = SomaService::new(
        SomaClient::new(&SomaConfig {
            api_url: String::new(),
            api_key: "test".into(),
            ..SomaConfig::default()
        })
        .expect("stub client should always build"),
    );
    let registry = soma_service::static_provider_registry(service.clone())
        .expect("static provider registry should always build");
    let runtime = runtime_for_components(service, registry, empty_gateway_product_state());
    AppState::new(
        McpConfig::default(),
        auth_policy,
        runtime,
        Default::default(),
    )
}

#[cfg(feature = "mcp")]
#[test]
fn maps_loopback_dev_policy_to_loopback_dev_mode() {
    let state = state(AuthPolicy::LoopbackDev);
    assert_eq!(authorization_mode(&state), AuthorizationMode::LoopbackDev);
}

#[cfg(feature = "mcp")]
#[test]
fn maps_trusted_gateway_policy_to_trusted_gateway_mode() {
    let state = state(AuthPolicy::TrustedGatewayUnscoped);
    assert_eq!(
        authorization_mode(&state),
        AuthorizationMode::TrustedGateway
    );
}

#[cfg(all(feature = "mcp", feature = "auth"))]
#[test]
fn maps_mounted_policy_to_mounted_mode() {
    let state = state(AuthPolicy::Mounted { auth_state: None });
    assert_eq!(authorization_mode(&state), AuthorizationMode::Mounted);
}

/// Reachability check for the PR 11 review fix: `runtime_for_components`
/// wires `soma_integrations::CodeModeApplicationPort` into `ApplicationPorts`
/// via `.with_codemode(...)`. Prove that wiring is live through the same
/// composition `apps/soma` actually uses (`state()` above), not just that
/// `CodeModeApplicationPort` works in its own crate's isolated unit tests.
/// Before the fix, `ApplicationPorts::unavailable()` left `codemode` on
/// `UnavailableEnginePort`, whose error code is always `"engine_unavailable"`
/// regardless of the request; asserting the code is something else (here,
/// `"codemode_disabled"`, since the wired port's default config is disabled)
/// proves a real `CodeModePort` is installed instead of the fallback.
#[cfg(feature = "mcp")]
#[tokio::test]
async fn codemode_port_is_wired_through_runtime_for_components_not_left_unavailable() {
    let state = state(AuthPolicy::LoopbackDev);
    let context = ExecutionContext::loopback(
        Surface::Mcp,
        RequestId::new("codemode-wiring-test").unwrap(),
    );
    let request = CodeModeExecuteRequest {
        source: "return 1;".to_owned(),
        input: serde_json::json!({}),
    };

    let error = state
        .application()
        .codemode_execute(request, context)
        .await
        .expect_err("default CodeModeApplicationPort config is disabled");

    assert_ne!(
        error.code, "engine_unavailable",
        "codemode port must not be the unwired UnavailableEnginePort fallback"
    );
    assert_eq!(error.code, "codemode_disabled");
}

#[cfg(feature = "cli")]
#[tokio::test]
async fn local_cli_composition_builds_the_application_catalog() {
    let providers = tempfile::tempdir().unwrap();
    std::fs::write(
        providers.path().join("fixture.json"),
        r#"{
          "schema_version": 1,
          "provider": { "name": "fixture", "kind": "static-rust" },
          "tools": [{
            "name": "fixture_action",
            "description": "Composition refresh fixture",
            "input_schema": { "type": "object", "properties": {}, "additionalProperties": false },
            "output_schema": { "type": "object", "properties": {}, "additionalProperties": true },
            "cli": { "enabled": true, "command": "fixture" }
          }]
        }"#,
    )
    .unwrap();
    let application = super::cli_application_with_provider_dir(
        &soma_config::Config::default(),
        Some(providers.path()),
    )
    .await
    .unwrap();

    assert_eq!(application.resolve_cli_action("status").unwrap(), "status");
    assert_eq!(
        application.provider_for_action("status").as_deref(),
        Some("static-rust")
    );
    assert_eq!(
        application.resolve_cli_action("fixture").unwrap(),
        "fixture_action"
    );
}

#[cfg(feature = "auth")]
#[test]
fn soma_auth_config_builder_supports_upstream_oauth_without_inbound_oauth() {
    let cfg = soma_integrations::auth::soma_auth_config_builder()
        .build_from_sources([
            (
                "SOMA_MCP_PUBLIC_URL".to_string(),
                "https://mcp.example.com".to_string(),
            ),
            (
                "SOMA_MCP_AUTH_SQLITE_PATH".to_string(),
                "/tmp/soma-auth.db".to_string(),
            ),
        ])
        .unwrap();

    assert!(matches!(cfg.mode, soma_auth::config::AuthMode::Bearer));
    assert_eq!(cfg.resource_path, "/mcp");
    assert!(cfg.scopes_supported.contains(&"soma:admin".to_string()));
}
