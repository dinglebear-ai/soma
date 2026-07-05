use std::{fs, path::Path};

use rtemplate_contracts::{
    provider_validation::validate_provider_manifest_value, providers::CapabilityGrant,
};
use rtemplate_service::{
    capabilities::CapabilityBroker,
    provider_registry::{
        ProviderAuthMode, ProviderCall, ProviderPrincipal, ProviderRegistry, ProviderRequestLimits,
        ProviderSurface,
    },
    providers::openapi::OpenApiProvider,
};
use serde_json::json;

fn openapi_catalog() -> rtemplate_contracts::providers::ProviderCatalog {
    let path =
        workspace_root().join("docs/contracts/examples/provider-manifests/openapi.valid.json");
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(path).expect("fixture should exist"))
            .expect("fixture JSON");
    validate_provider_manifest_value(&value).expect("valid OpenAPI fixture")
}

#[tokio::test]
async fn openapi_provider_network_is_default_denied_before_execution() {
    let provider = OpenApiProvider::arc(openapi_catalog());
    let registry = ProviderRegistry::new(vec![provider]).expect("registry");
    let error = registry
        .dispatch(call())
        .await
        .expect_err("network should be denied before provider code");

    assert_eq!(&*error.code, "capability_denied");
}

#[tokio::test]
async fn openapi_provider_execution_is_blocked_until_ssrf_tests_land() {
    let provider = OpenApiProvider::arc(openapi_catalog());
    let registry = ProviderRegistry::with_capabilities(
        vec![provider],
        CapabilityBroker::new(vec![CapabilityGrant::Network {
            allowed_hosts: vec!["api.weather.example".to_owned()],
        }]),
    )
    .expect("registry");
    let error = registry
        .dispatch(call())
        .await
        .expect_err("execution is deferred");

    assert_eq!(&*error.code, "openapi_provider_execution_deferred");
    assert!(error.message.contains("blocked"));
}

fn call() -> ProviderCall {
    ProviderCall {
        provider: String::new(),
        action: "weather-current".to_owned(),
        params: json!({"city": "Paris"}),
        principal: ProviderPrincipal::loopback_dev(),
        auth_mode: ProviderAuthMode::LoopbackDev,
        surface: ProviderSurface::Mcp,
        destructive_confirmed: false,
        limits: ProviderRequestLimits::default(),
        snapshot_id: String::new(),
    }
}

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}
