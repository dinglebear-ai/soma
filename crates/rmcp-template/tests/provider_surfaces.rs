use rmcp_template::testing::loopback_state;
use rtemplate_service::provider_registry::{
    ProviderAuthMode, ProviderCall, ProviderPrincipal, ProviderRequestLimits, ProviderSurface,
};
use serde_json::json;

#[tokio::test]
async fn static_provider_preserves_builtin_service_outputs() {
    let state = loopback_state();
    let output = state
        .provider_registry
        .dispatch(ProviderCall {
            provider: String::new(),
            action: "greet".to_owned(),
            params: json!({"name": "Alice"}),
            principal: ProviderPrincipal::loopback_dev(),
            auth_mode: ProviderAuthMode::LoopbackDev,
            surface: ProviderSurface::Mcp,
            destructive_confirmed: false,
            limits: ProviderRequestLimits::default(),
            snapshot_id: String::new(),
        })
        .await
        .expect("greet dispatch");

    assert_eq!(output.value["greeting"], "Hello, Alice!");
}

#[tokio::test]
async fn static_provider_help_is_public_and_rest_exposed() {
    let state = loopback_state();
    let output = state
        .provider_registry
        .dispatch(ProviderCall {
            provider: String::new(),
            action: "help".to_owned(),
            params: json!({}),
            principal: ProviderPrincipal::loopback_dev(),
            auth_mode: ProviderAuthMode::Mounted,
            surface: ProviderSurface::Rest,
            destructive_confirmed: false,
            limits: ProviderRequestLimits::default(),
            snapshot_id: String::new(),
        })
        .await
        .expect("help dispatch");

    assert_eq!(output.value["preferred_rest_style"], "direct_routes");
}
