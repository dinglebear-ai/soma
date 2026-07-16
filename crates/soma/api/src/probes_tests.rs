use soma_contracts::config::{McpConfig, SomaConfig};
use soma_runtime::server::{empty_gateway_product_state, AppState, AuthPolicy};
use soma_service::{static_provider_registry, SomaClient, SomaService};

use super::status_body;

fn loopback_state() -> AppState {
    let client = SomaClient::new(&SomaConfig {
        api_url: String::new(),
        api_key: "test".into(),
        ..SomaConfig::default()
    })
    .expect("stub client should build");
    let service = SomaService::new(client);
    let provider_registry = static_provider_registry(service.clone()).expect("static registry");

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

#[test]
fn status_body_is_local_and_redacted() {
    let body = status_body(&loopback_state());

    assert_eq!(body["status"], "ok");
    assert_eq!(body["server"], "soma");
    assert_eq!(body["transport"], "http");
    assert!(body.get("api_url").is_none(), "{body}");
    assert!(body.get("api_key").is_none(), "{body}");
}
