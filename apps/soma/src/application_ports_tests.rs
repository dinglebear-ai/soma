use super::authorization_mode;
use soma_application::{CodeModeExecuteRequest, ExecutionContext};
use soma_client::SomaClient;
use soma_contracts::config::{McpConfig, SomaConfig};
use soma_domain::{AuthorizationMode, RequestId, Surface};
use soma_runtime::server::{empty_gateway_product_state, AppState, AuthPolicy};
use soma_service::SomaService;

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
    let runtime = super::runtime_for_components(service, registry, empty_gateway_product_state());
    AppState::new(
        McpConfig::default(),
        auth_policy,
        runtime,
        Default::default(),
    )
}

#[test]
fn maps_loopback_dev_policy_to_loopback_dev_mode() {
    let state = state(AuthPolicy::LoopbackDev);
    assert_eq!(authorization_mode(&state), AuthorizationMode::LoopbackDev);
}

#[test]
fn maps_trusted_gateway_policy_to_trusted_gateway_mode() {
    let state = state(AuthPolicy::TrustedGatewayUnscoped);
    assert_eq!(
        authorization_mode(&state),
        AuthorizationMode::TrustedGateway
    );
}

#[cfg(feature = "auth")]
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
