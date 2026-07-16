use async_trait::async_trait;
use serde_json::{Value, json};
use soma_provider_core::{
    Provider, ProviderCall, ProviderCatalog, ProviderError, ProviderId, ProviderLimits,
    ProviderManifest, ProviderOutput, ProviderRegistry, ToolSpec,
};

#[derive(Clone)]
struct FakeProvider {
    catalog: ProviderCatalog,
    output: Result<Value, ProviderError>,
}

#[async_trait]
impl Provider for FakeProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, _call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        self.output.clone().map(ProviderOutput::value)
    }
}

fn provider(tool: ToolSpec, output: Result<Value, ProviderError>) -> FakeProvider {
    FakeProvider {
        catalog: ProviderManifest::new(
            ProviderId::new("dispatch-provider").unwrap(),
            "Dispatch provider",
            "1.0.0",
        )
        .with_tool(tool),
        output,
    }
}

fn registry(tool: ToolSpec, output: Result<Value, ProviderError>) -> ProviderRegistry {
    ProviderRegistry::builder()
        .register(provider(tool, output))
        .unwrap()
        .build()
        .unwrap()
}

#[test]
fn invalid_declared_input_and_output_schemas_are_rejected_at_build() {
    let mut invalid_input = ToolSpec::new(
        "run",
        "run",
        json!({
            "type": "object",
            "properties": {"value": {"type": "string", "pattern": "["}}
        }),
    );
    let input_error = ProviderRegistry::builder()
        .register(provider(invalid_input.clone(), Ok(Value::Null)))
        .unwrap()
        .build()
        .err()
        .expect("invalid input schema must fail");
    assert_eq!(input_error.code(), "input_schema_invalid");

    invalid_input.input_schema = json!({"type": "object"});
    invalid_input.output_schema = Some(json!({
        "type": "object",
        "properties": {"value": {"type": "string", "pattern": "["}}
    }));
    let output_error = ProviderRegistry::builder()
        .register(provider(invalid_input, Ok(Value::Null)))
        .unwrap()
        .build()
        .err()
        .expect("invalid output schema must fail");
    assert_eq!(output_error.code(), "output_schema_invalid");
}

#[tokio::test]
async fn dispatch_enforces_input_and_output_schemas() {
    let mut tool = ToolSpec::new(
        "run",
        "run",
        json!({"type": "object", "required": ["message"]}),
    );
    tool.output_schema = Some(json!({"type": "object", "required": ["result"]}));
    let registry = registry(tool, Ok(json!({"wrong": true})));

    let input_error = registry
        .dispatch(ProviderCall::new("run", json!({})))
        .await
        .expect_err("invalid input must fail");
    assert_eq!(input_error.code.as_ref(), "input_schema_failed");

    let output_error = registry
        .dispatch(ProviderCall::new("run", json!({"message": "hello"})))
        .await
        .expect_err("invalid output must fail");
    assert_eq!(output_error.code.as_ref(), "output_schema_failed");
}

#[tokio::test]
async fn dispatch_enforces_declared_input_and_output_limits() {
    let mut tool = ToolSpec::new("run", "run", json!({"type": "object"}));
    tool.limits = Some(ProviderLimits {
        timeout_ms: None,
        max_response_bytes: Some(1),
        max_input_bytes: Some(1),
    });
    let input_registry = registry(tool, Ok(json!({})));

    let input_error = input_registry
        .dispatch(ProviderCall::new("run", json!({})))
        .await
        .expect_err("oversized input must fail");
    assert_eq!(input_error.code.as_ref(), "input_too_large");

    let mut tool = ToolSpec::new("run", "run", json!({"type": "object"}));
    tool.limits = Some(ProviderLimits {
        timeout_ms: None,
        max_response_bytes: Some(1),
        max_input_bytes: Some(1024),
    });
    let registry = registry(tool, Ok(json!({})));
    let output_error = registry
        .dispatch(ProviderCall::new("run", json!({})))
        .await
        .expect_err("oversized output must fail");
    assert_eq!(output_error.code.as_ref(), "response_too_large");
}

#[tokio::test]
async fn dispatch_reports_unknown_actions_and_preserves_provider_failures() {
    let provider_error = ProviderError::execution("dispatch-provider", "run", "boom");
    let registry = registry(
        ToolSpec::new("run", "run", json!({"type": "object"})),
        Err(provider_error.clone()),
    );

    let unknown = registry
        .dispatch(ProviderCall::new("missing", json!({})))
        .await
        .expect_err("unknown action must fail");
    assert_eq!(unknown.code.as_ref(), "unknown_action");

    let failure = registry
        .dispatch(ProviderCall::new("run", json!({})))
        .await
        .expect_err("provider failure must pass through");
    assert_eq!(failure, provider_error);
}
