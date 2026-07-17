use super::authorization_mode;
use soma_contracts::config::{McpConfig, SomaConfig};
use soma_domain::AuthorizationMode;
use soma_runtime::server::{empty_gateway_product_state, AppState, AuthPolicy};
use soma_service::{SomaClient, SomaService};

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
