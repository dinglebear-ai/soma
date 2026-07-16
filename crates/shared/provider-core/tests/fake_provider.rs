use async_trait::async_trait;
use serde_json::{Value, json};
use soma_provider_core::{
    Provider, ProviderCall, ProviderCatalog, ProviderError, ProviderId, ProviderManifest,
    ProviderOutput, ProviderRegistry, ToolSpec, validate_provider_manifest,
};

struct FakeProvider {
    catalog: ProviderCatalog,
}

impl FakeProvider {
    fn new() -> Self {
        let manifest = ProviderManifest::new(
            ProviderId::new("fake-provider").expect("valid provider id"),
            "Fake provider",
            "1.0.0",
        );
        let echo = ToolSpec::new(
            "echo",
            "Echo a message",
            json!({
                "type": "object",
                "properties": { "message": { "type": "string" } },
                "required": ["message"],
                "additionalProperties": false
            }),
        );

        Self {
            catalog: manifest.with_tool(echo),
        }
    }
}

#[test]
fn optional_manifest_metadata_round_trips_with_legacy_null_defaults() {
    let manifest: ProviderManifest = serde_json::from_value(json!({
        "schema_version": 1,
        "provider": {
            "name": "fake-provider",
            "kind": "static-rust",
            "title": "Fake provider",
            "version": "1.0.0"
        }
    }))
    .expect("minimal manifest deserializes");

    let serialized = serde_json::to_value(&manifest).expect("manifest serializes");
    assert!(serialized["meta"].is_null());
    assert!(serialized["capabilities"].is_object());
    validate_provider_manifest(&manifest).expect("typed defaults remain compatibility-valid");
}

#[async_trait]
impl Provider for FakeProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        match call.tool() {
            "echo" => Ok(ProviderOutput::value(json!({
                "message": call
                    .arguments()
                    .get("message")
                    .and_then(Value::as_str)
                    .expect("registry validated the request")
            }))),
            tool => Err(ProviderError::tool_not_found(tool)),
        }
    }
}

#[tokio::test]
async fn standalone_provider_registers_snapshots_and_dispatches_without_soma_types() {
    let registry = ProviderRegistry::builder()
        .register(FakeProvider::new())
        .expect("fake provider registers")
        .build()
        .expect("registry builds");

    let snapshot = registry.snapshot();
    assert_eq!(snapshot.provider_count(), 1);
    assert_eq!(
        snapshot.tool("echo").unwrap().provider_id().as_str(),
        "fake-provider"
    );
    assert!(!snapshot.fingerprint().as_str().is_empty());

    let output = registry
        .dispatch(ProviderCall::new("echo", json!({ "message": "hello" })))
        .await
        .expect("fake provider dispatches");

    assert_eq!(output.into_value(), json!({ "message": "hello" }));
}
