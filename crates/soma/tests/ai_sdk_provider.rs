use std::{fs, time::Duration};

use serde_json::json;
use soma_contracts::config::SomaConfig;
use soma_service::{
    dynamic_provider_registry_from_dir, provider_registry::ProviderAuthMode,
    provider_registry::ProviderCall, provider_registry::ProviderPrincipal,
    provider_registry::ProviderRequestLimits, provider_registry::ProviderSurface, SomaClient,
    SomaService,
};

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

#[tokio::test]
async fn ai_sdk_provider_passes_provider_level_environment() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let providers = temp.path().join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("env-tool.ts"),
        r#"
export default {
  "schema_version": 1,
  "provider": {
    "name": "env-ai",
    "kind": "ai-sdk"
  },
  "env": [{
    "name": "AI_SDK_PROVIDER_SECRET",
    "server_prefixed": false,
    "required": true,
    "sensitive": true,
    "default": "allowed"
  }],
  "tools": [{
    "name": "read_ai_env",
    "description": "Read declared and undeclared environment.",
    "input_schema": {
      "type": "object",
      "additionalProperties": false,
      "properties": {}
    }
  }]
};

export async function call() {
  return {
    declared: process.env.AI_SDK_PROVIDER_SECRET,
    undeclared_path: process.env.PATH ?? "missing"
  };
}
"#,
    )?;

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let output = dispatch(&registry, "read_ai_env", json!({})).await?;

    assert_eq!(
        output,
        json!({"declared": "allowed", "undeclared_path": "missing"})
    );
    Ok(())
}

#[tokio::test]
async fn ai_sdk_provider_kills_timed_out_process() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let providers = temp.path().join("providers");
    fs::create_dir(&providers)?;
    fs::write(
        providers.join("slow-tool.ts"),
        r#"
export default {
  "schema_version": 1,
  "provider": {
    "name": "timeout-ai",
    "kind": "ai-sdk"
  },
  "tools": [{
    "name": "slow_ai",
    "description": "Sleep long enough to exceed the configured timeout.",
    "input_schema": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "marker": {"type": "string"}
      },
      "required": ["marker"]
    },
    "limits": {"timeout_ms": 100}
  }]
};

export async function call(input) {
  await new Promise((resolve) => setTimeout(resolve, 500));
  const { writeFileSync } = await import("node:fs");
  writeFileSync(input.params.marker, "still running");
  return {done: true};
}
"#,
    )?;
    let marker = temp.path().join("timed-out-ai-sdk.txt");

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let result = dispatch(&registry, "slow_ai", json!({"marker": marker})).await;

    assert!(result.is_err(), "slow AI SDK provider call should time out");
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(
        !marker.exists(),
        "timed-out AI SDK sidecar should not continue running after timeout"
    );
    Ok(())
}

async fn dispatch(
    registry: &soma_service::ProviderRegistry,
    action: &str,
    params: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let output = registry
        .dispatch(ProviderCall {
            provider: String::new(),
            action: action.to_owned(),
            params,
            principal: ProviderPrincipal::loopback_dev(),
            auth_mode: ProviderAuthMode::LoopbackDev,
            surface: ProviderSurface::Mcp,
            destructive_confirmed: false,
            limits: ProviderRequestLimits::default(),
            snapshot_id: String::new(),
        })
        .await?;
    Ok(output.value)
}

fn service() -> anyhow::Result<SomaService> {
    let client = SomaClient::new(&SomaConfig {
        api_url: String::new(),
        api_key: "test".to_owned(),
    })?;
    Ok(SomaService::new(client))
}
