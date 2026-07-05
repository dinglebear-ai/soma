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

fn wasm_provider() -> anyhow::Result<Vec<u8>> {
    let mut bytes = wat::parse_str(
        r#"
(module
  (memory (export "memory") 1)
  (global $input_ptr (mut i32) (i32.const 1024))
  (global $output_ptr (mut i32) (i32.const 2048))
  (global $output_len (mut i32) (i32.const 28))
  (func (export "rtemplate_input_alloc") (param $len i32) (result i32)
    (global.set $input_ptr (i32.const 1024))
    (global.get $input_ptr))
  (func (export "rtemplate_input_ptr") (result i32)
    (global.get $input_ptr))
  (func (export "rtemplate_call") (result i32)
    (i32.const 0))
  (func (export "rtemplate_output_ptr") (result i32)
    (global.get $output_ptr))
  (func (export "rtemplate_output_len") (result i32)
    (global.get $output_len))
  (data (i32.const 2048) "{\"ok\":true,\"runtime\":\"wasm\"}"))
"#,
    )?;
    append_provider_manifest(&mut bytes, provider_manifest().as_bytes());
    Ok(bytes)
}

fn provider_manifest() -> String {
    json!({
        "schema_version": 1,
        "provider": {
            "name": "live-wasm",
            "kind": "wasm"
        },
        "tools": [{
            "name": "live_wasm_exec",
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

fn append_provider_manifest(bytes: &mut Vec<u8>, manifest: &[u8]) {
    let name = b"rtemplate.provider";
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

fn service() -> anyhow::Result<ExampleService> {
    let client = ExampleClient::new(&ExampleConfig {
        api_url: String::new(),
        api_key: "test".to_owned(),
    })?;
    Ok(ExampleService::new(client))
}
