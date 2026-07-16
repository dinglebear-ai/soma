use async_trait::async_trait;
use serde_json::{Value, json};
use soma_provider_core::{
    Provider, ProviderCall, ProviderCatalog, ProviderError, ProviderId, ProviderManifest,
    ProviderOutput, ProviderRegistry, ToolSpec,
};

#[derive(Clone)]
struct FakeProvider(ProviderCatalog);

#[async_trait]
impl Provider for FakeProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.0.clone()
    }

    async fn call(&self, _call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        Ok(ProviderOutput::value(Value::Null))
    }
}

fn provider(name: &str, action: &str) -> FakeProvider {
    FakeProvider(
        ProviderManifest::new(ProviderId::new(name).unwrap(), name, "1.0.0")
            .with_tool(ToolSpec::new(action, action, json!({"type": "object"}))),
    )
}

#[test]
fn fingerprints_are_independent_of_registration_order() {
    let forward = ProviderRegistry::builder()
        .register(provider("alpha-provider", "alpha"))
        .unwrap()
        .register(provider("beta-provider", "beta"))
        .unwrap()
        .build()
        .unwrap();
    let reverse = ProviderRegistry::builder()
        .register(provider("beta-provider", "beta"))
        .unwrap()
        .register(provider("alpha-provider", "alpha"))
        .unwrap()
        .build()
        .unwrap();

    assert_eq!(
        forward.snapshot().fingerprint(),
        reverse.snapshot().fingerprint()
    );
}
