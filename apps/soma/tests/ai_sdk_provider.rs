use std::{fs, process::Command, time::Duration};

use serde_json::{json, Value};
use soma_client::SomaClient;
use soma_config::SomaConfig;
use soma_service::{
    dynamic_provider_registry_from_dir, provider_registry::ProviderAuthMode,
    provider_registry::ProviderCall, provider_registry::ProviderPrincipal,
    provider_registry::ProviderRequestLimits, provider_registry::ProviderSurface, SomaService,
};

#[tokio::test]
async fn ai_sdk_provider_executes_hot_dropped_typescript_handler() -> anyhow::Result<()> {
    if !node_sidecar_available() {
        return Ok(());
    }

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
    message: input.params.message,
    envelope: input
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
    assert_provider_envelope(
        &output.value["envelope"],
        "live-ai",
        "live_ts_exec",
        json!({"message": "hello"}),
    );
    Ok(())
}

#[tokio::test]
async fn ai_sdk_provider_passes_provider_level_environment() -> anyhow::Result<()> {
    if !node_sidecar_available() {
        return Ok(());
    }

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
    if !node_sidecar_available() {
        return Ok(());
    }

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
        ..SomaConfig::default()
    })?;
    Ok(SomaService::new(client))
}

fn assert_provider_envelope(envelope: &Value, provider: &str, action: &str, params: Value) {
    assert_eq!(envelope["schema_version"], 1);
    assert_eq!(envelope["provider"], provider);
    assert_eq!(envelope["action"], action);
    assert_eq!(envelope["params"], params);
    assert_eq!(envelope["surface"], "mcp");
    assert!(
        envelope["snapshot_id"]
            .as_str()
            .is_some_and(|snapshot_id| snapshot_id.starts_with("sha256:")),
        "snapshot_id should be the active provider snapshot fingerprint"
    );
}

fn node_sidecar_available() -> bool {
    let output = Command::new("node")
        .args([
            "--input-type=module",
            "--eval",
            "import { readFileSync } from 'node:fs'; console.log(JSON.stringify({ok: true}));",
        ])
        .output();
    match output {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            eprintln!(
                "skipping AI SDK provider smoke: node sidecar probe failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            false
        }
        Err(error) => {
            eprintln!("skipping AI SDK provider smoke: node sidecar unavailable: {error}");
            false
        }
    }
}
