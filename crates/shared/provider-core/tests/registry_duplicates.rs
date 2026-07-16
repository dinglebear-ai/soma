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

fn provider(name: &str, tool: ToolSpec) -> FakeProvider {
    FakeProvider(
        ProviderManifest::new(ProviderId::new(name).unwrap(), name, "1.0.0").with_tool(tool),
    )
}

fn tool(name: &str) -> ToolSpec {
    ToolSpec::new(name, name, json!({"type": "object"}))
}

#[test]
fn duplicate_provider_ids_are_rejected() {
    let builder = ProviderRegistry::builder()
        .register(provider("duplicate", tool("first")))
        .unwrap();
    let error = match builder.register(provider("duplicate", tool("second"))) {
        Ok(_) => panic!("duplicate provider must fail"),
        Err(error) => error,
    };
    assert_eq!(error.code(), "duplicate_provider_name");
}

#[test]
fn duplicate_tool_names_across_providers_are_rejected() {
    let error = ProviderRegistry::builder()
        .register(provider("first-provider", tool("shared")))
        .unwrap()
        .register(provider("second-provider", tool("shared")))
        .unwrap()
        .build()
        .err()
        .expect("duplicate tool must fail");
    assert_eq!(error.code(), "duplicate_tool_name");
}

#[test]
fn duplicate_rest_routes_and_cli_aliases_are_rejected() {
    let first: ToolSpec = serde_json::from_value(json!({
        "name": "first",
        "description": "first",
        "input_schema": {"type": "object"},
        "rest": {"enabled": true, "method": "POST", "path": "/v1/shared"},
        "cli": {"enabled": true, "command": "first", "aliases": ["shared"]}
    }))
    .unwrap();
    let second: ToolSpec = serde_json::from_value(json!({
        "name": "second",
        "description": "second",
        "input_schema": {"type": "object"},
        "rest": {"enabled": true, "method": "POST", "path": "/v1/shared"},
        "cli": {"enabled": true, "command": "second", "aliases": ["shared"]}
    }))
    .unwrap();

    let route_error = ProviderRegistry::builder()
        .register(provider("first-provider", first.clone()))
        .unwrap()
        .register(provider("second-provider", second.clone()))
        .unwrap()
        .build()
        .err()
        .expect("duplicate route must fail");
    assert_eq!(route_error.code(), "duplicate_rest_route");

    let mut second_without_route = second;
    second_without_route.rest = None;
    let alias_error = ProviderRegistry::builder()
        .register(provider("first-provider", first))
        .unwrap()
        .register(provider("second-provider", second_without_route))
        .unwrap()
        .build()
        .err()
        .expect("duplicate alias must fail");
    assert_eq!(alias_error.code(), "duplicate_cli_command");
}

#[test]
fn duplicate_primitive_names_across_kinds_are_rejected() {
    let mut catalog = ProviderManifest::new(
        ProviderId::new("primitive-provider").unwrap(),
        "Primitive provider",
        "1.0.0",
    );
    catalog.prompts.push(
        serde_json::from_value(json!({
            "name": "shared",
            "description": "prompt"
        }))
        .unwrap(),
    );
    catalog.resources.push(
        serde_json::from_value(json!({
            "uri_template": "provider://shared",
            "name": "shared",
            "description": "resource"
        }))
        .unwrap(),
    );

    let error = ProviderRegistry::builder()
        .register(FakeProvider(catalog))
        .err()
        .expect("duplicate primitive must fail");
    assert_eq!(error.code(), "duplicate_mcp_primitive");
}
