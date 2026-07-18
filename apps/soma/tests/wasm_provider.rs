use std::fs;

use serde_json::{json, Value};
use soma_application::{
    dynamic_provider_registry_from_dir, provider_registry::ProviderAuthMode,
    provider_registry::ProviderCall, provider_registry::ProviderPrincipal,
    provider_registry::ProviderRequestLimits, provider_registry::ProviderSurface, SomaService,
};
use soma_client::SomaClient;
use soma_config::SomaConfig;

#[tokio::test]
async fn wasm_provider_executes_hot_dropped_module() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let providers = temp.path().join("providers");
    fs::create_dir(&providers)?;
    fs::write(providers.join("live-wasm.wasm"), wasm_provider()?)?;

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let output = registry
        .dispatch(ProviderCall {
            provider: String::new(),
            action: "live_wasm_exec".to_owned(),
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
    assert_eq!(output.value["runtime"], "wasm");
    Ok(())
}

#[tokio::test]
async fn wasm_provider_receives_execution_envelope() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let providers = temp.path().join("providers");
    fs::create_dir(&providers)?;
    fs::write(providers.join("echo-envelope.wasm"), wasm_echo_provider()?)?;

    let registry = dynamic_provider_registry_from_dir(service()?, &providers)?;
    let output = registry
        .dispatch(ProviderCall {
            provider: String::new(),
            action: "echo_wasm_envelope".to_owned(),
            params: json!({"message": "hello"}),
            principal: ProviderPrincipal::loopback_dev(),
            auth_mode: ProviderAuthMode::LoopbackDev,
            surface: ProviderSurface::Mcp,
            destructive_confirmed: false,
            limits: ProviderRequestLimits::default(),
            snapshot_id: String::new(),
        })
        .await?;

    assert_provider_envelope(
        &output.value,
        "echo-wasm",
        "echo_wasm_envelope",
        json!({"message": "hello"}),
    );
    Ok(())
}

fn wasm_provider() -> anyhow::Result<Vec<u8>> {
    let mut bytes = wat::parse_str(
        r#"
(module
  (memory (export "memory") 1)
  (global $input_ptr (mut i32) (i32.const 1024))
  (global $output_ptr (mut i32) (i32.const 2048))
  (global $output_len (mut i32) (i32.const 28))
  (func (export "soma_input_alloc") (param $len i32) (result i32)
    (global.set $input_ptr (i32.const 1024))
    (global.get $input_ptr))
  (func (export "soma_input_ptr") (result i32)
    (global.get $input_ptr))
  (func (export "soma_call") (result i32)
    (i32.const 0))
  (func (export "soma_output_ptr") (result i32)
    (global.get $output_ptr))
  (func (export "soma_output_len") (result i32)
    (global.get $output_len))
  (data (i32.const 2048) "{\"ok\":true,\"runtime\":\"wasm\"}"))
"#,
    )?;
    append_provider_manifest(&mut bytes, provider_manifest().as_bytes());
    Ok(bytes)
}

fn wasm_echo_provider() -> anyhow::Result<Vec<u8>> {
    let mut bytes = wat::parse_str(
        r#"
(module
  (memory (export "memory") 1)
  (global $input_len (mut i32) (i32.const 0))
  (func (export "soma_input_alloc") (param $len i32) (result i32)
    (global.set $input_len (local.get $len))
    (i32.const 1024))
  (func (export "soma_input_ptr") (result i32)
    (i32.const 1024))
  (func (export "soma_call") (result i32)
    (i32.const 0))
  (func (export "soma_output_ptr") (result i32)
    (i32.const 1024))
  (func (export "soma_output_len") (result i32)
    (global.get $input_len)))
"#,
    )?;
    append_provider_manifest(
        &mut bytes,
        provider_manifest_for("echo-wasm", "echo_wasm_envelope").as_bytes(),
    );
    Ok(bytes)
}

fn provider_manifest() -> String {
    provider_manifest_for("live-wasm", "live_wasm_exec")
}

fn provider_manifest_for(name: &str, action: &str) -> String {
    json!({
        "schema_version": 1,
        "provider": {
            "name": name,
            "kind": "wasm"
        },
        "tools": [{
            "name": action,
            "description": "Execute a live WASM module.",
            "input_schema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "message": { "type": "string" }
                }
            },
            "limits": {
                "timeout_ms": 1000,
                "max_input_bytes": 4096,
                "max_response_bytes": 4096
            }
        }]
    })
    .to_string()
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

fn append_provider_manifest(bytes: &mut Vec<u8>, manifest: &[u8]) {
    let name = b"soma.provider";
    let mut payload = Vec::new();
    write_leb(name.len() as u32, &mut payload);
    payload.extend_from_slice(name);
    payload.extend_from_slice(manifest);

    bytes.push(0);
    write_leb(payload.len() as u32, bytes);
    bytes.extend(payload);
}

fn write_leb(mut value: u32, bytes: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn service() -> anyhow::Result<SomaService> {
    let client = SomaClient::new(&SomaConfig {
        api_url: String::new(),
        api_key: "test".to_owned(),
        ..SomaConfig::default()
    })?;
    Ok(SomaService::new(client))
}
