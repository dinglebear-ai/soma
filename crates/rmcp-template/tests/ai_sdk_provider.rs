use std::fs;

use rtemplate_contracts::config::ExampleConfig;
use rtemplate_service::{
    dynamic_provider_registry_from_dir, provider_registry::ProviderAuthMode,
    provider_registry::ProviderCall, provider_registry::ProviderPrincipal,
    provider_registry::ProviderRequestLimits, provider_registry::ProviderSurface, ExampleClient,
    ExampleService,
};
use serde_json::json;

#[tokio::test]
async fn ai_sdk_provider_executes_hot_dropped_typescript_handler() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let providers = temp.path().join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("live-tool.ts"),
        r#"
export default {
  "schema_version": 1,
  "provider": { "name": "live-ai", "kind": "ai-sdk" },
  "tools": [{
    "name": "live_ts_exec",
    "description": "Execute a live TypeScript handler.",
    "input_schema": {
      "type": "object",
      "additionalProperties": false,
      "properties": { "message": { "type": "string" } }
    }
  }]
};

export async function call(input) {
  return {
    ok: true,
    runtime: "typescript",
    action: input.action,
    message: input.params.message
  };
}
"#,
    )?;

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let output = registry
        .dispatch(ProviderCall {
            provider: String::new(),
            action: "live_ts_exec".to_owned(),
            params: json!({"message": "hello"}),
            principal: ProviderPrincipal::loopback_dev(),
            auth_mode: ProviderAuthMode::LoopbackDev,
            surface: ProviderSurface::Mcp,
            destructive_confirmed: false,
            limits: ProviderRequestLimits::default(),
            snapshot_id: String::new(),
        })
        .await?;

    assert_eq!(output.value["ok"], true);
    assert_eq!(output.value["runtime"], "typescript");
    assert_eq!(output.value["action"], "live_ts_exec");
    assert_eq!(output.value["message"], "hello");
    Ok(())
}

fn service() -> anyhow::Result<ExampleService> {
    let client = ExampleClient::new(&ExampleConfig {
        api_url: String::new(),
        api_key: "test".to_owned(),
    })?;
    Ok(ExampleService::new(client))
}
