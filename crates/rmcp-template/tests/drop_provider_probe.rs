use std::{fs, process::Stdio};

use rmcp::{
    model::CallToolRequestParams,
    transport::{ConfigureCommandExt, TokioChildProcess},
    ServiceExt,
};
use serde_json::{json, Map, Value};
use tokio::process::Command;

async fn stdio_client_in(
    cwd: &std::path::Path,
) -> anyhow::Result<rmcp::service::RunningService<rmcp::RoleClient, ()>> {
    let binary = env!("CARGO_BIN_EXE_rtemplate");
    let (transport, _stderr) = TokioChildProcess::builder(Command::new(binary).configure(|cmd| {
        cmd.arg("mcp")
            .current_dir(cwd)
            .env("RUST_LOG", "warn")
            .env_remove("RTEMPLATE_API_URL")
            .env_remove("RTEMPLATE_API_KEY")
            .env_remove("RTEMPLATE_MCP_TOKEN")
            .env_remove("RTEMPLATE_PROVIDER_DIR");
    }))
    .stderr(Stdio::piped())
    .spawn()?;
    Ok(().serve(transport).await?)
}

fn action_enum(schema: &Map<String, Value>) -> Vec<String> {
    schema["properties"]["action"]["enum"]
        .as_array()
        .expect("action enum should exist")
        .iter()
        .map(|value| value.as_str().expect("enum value").to_owned())
        .collect()
}

#[tokio::test]
async fn dropped_ts_and_wasm_files_hot_register_provider_tools() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let providers = temp.path().join("providers");
    fs::create_dir(&providers)?;

    let service = stdio_client_in(temp.path()).await?;
    let before = service.list_tools(Default::default()).await?;
    let before_actions = action_enum(&before.tools[0].input_schema);
    println!("before_actions={before_actions:?}");
    assert!(!before_actions.contains(&"live_ts_probe".to_owned()));
    assert!(!before_actions.contains(&"live_wasm_probe".to_owned()));

    fs::write(
        providers.join("live-ai-sdk.ts"),
        format!(
            "export default {};\n",
            provider_manifest("live-ai-sdk", "ai-sdk", "live_ts_probe")
        ),
    )?;
    fs::write(
        providers.join("live-wasm-provider.wasm"),
        wasm_provider(
            provider_manifest("live-wasm-provider", "wasm", "live_wasm_probe").as_bytes(),
        ),
    )?;
    println!("dropped_files={}", providers.display());

    let after = service.list_tools(Default::default()).await?;
    let after_actions = action_enum(&after.tools[0].input_schema);
    println!("after_actions={after_actions:?}");
    assert!(after_actions.contains(&"live_ts_probe".to_owned()));
    assert!(after_actions.contains(&"live_wasm_probe".to_owned()));

    for action in ["live_ts_probe", "live_wasm_probe"] {
        let result = service
            .call_tool(
                CallToolRequestParams::new("example")
                    .with_arguments(json!({"action": action}).as_object().unwrap().clone()),
            )
            .await?;
        let structured = result
            .structured_content
            .expect("dynamic provider call should return structured content");
        println!("{action}_result={structured}");
        assert_eq!(structured["ok"], true);
        assert_eq!(structured["action"], action);
    }

    service.cancel().await?;
    Ok(())
}

fn provider_manifest(name: &str, kind: &str, action: &str) -> String {
    json!({
        "schema_version": 1,
        "provider": {
            "name": name,
            "kind": kind,
            "enabled": true
        },
        "tools": [{
            "name": action,
            "description": format!("Live provider action {action}"),
            "input_schema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {}
            },
            "meta": {
                "result": {
                    "ok": true,
                    "action": action
                }
            }
        }]
    })
    .to_string()
}

fn wasm_provider(manifest: &[u8]) -> Vec<u8> {
    let name = b"rtemplate.provider";
    let mut payload = Vec::new();
    write_leb(name.len() as u32, &mut payload);
    payload.extend_from_slice(name);
    payload.extend_from_slice(manifest);

    let mut bytes = b"\0asm\x01\0\0\0".to_vec();
    bytes.push(0);
    write_leb(payload.len() as u32, &mut bytes);
    bytes.extend(payload);
    bytes
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
