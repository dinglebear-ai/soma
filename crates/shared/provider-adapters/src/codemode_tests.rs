use serde_json::json;
use soma_provider_core::{ProviderCall, ProviderManifest};

use super::*;

fn catalog() -> ProviderCatalog {
    serde_json::from_value::<ProviderManifest>(json!({
        "schema_version": 1,
        "provider": { "name": "codemode-fixture", "kind": "static-rust" },
        "tools": [{
            "name": "run",
            "description": "Run a fixed snippet.",
            "input_schema": {"type": "object"},
        }],
    }))
    .expect("fixture manifest deserializes")
}

#[tokio::test]
async fn executes_the_configured_snippet_and_returns_its_result() {
    let provider = CodeModeSnippetProvider::new(
        catalog(),
        "return 1 + 1;",
        CodeModeConfig {
            enabled: true,
            ..CodeModeConfig::default()
        },
    );
    let output = provider
        .call(ProviderCall::new("run", json!({})))
        .await
        .expect("snippet executes");
    assert_eq!(output.value["result"], json!(2));
}
