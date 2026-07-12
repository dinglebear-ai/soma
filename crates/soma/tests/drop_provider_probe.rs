use std::{
    fs,
    net::TcpListener,
    process::Stdio,
    time::{Duration, Instant},
};

use rmcp::{
    model::CallToolRequestParams,
    transport::{ConfigureCommandExt, TokioChildProcess},
    ServiceExt,
};
use serde_json::{json, Map, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
};

async fn stdio_client_in(
    cwd: &std::path::Path,
) -> anyhow::Result<rmcp::service::RunningService<rmcp::RoleClient, ()>> {
    let binary = env!("CARGO_BIN_EXE_soma");
    let (transport, _stderr) = TokioChildProcess::builder(Command::new(binary).configure(|cmd| {
        cmd.arg("mcp")
            .current_dir(cwd)
            .env("HOME", cwd)
            .env("SOMA_HOME", cwd)
            .env("SOMA_API_URL", "")
            .env("SOMA_API_KEY", "")
            .env("RUST_LOG", "warn")
            .env("SOMA_MCP_TOKEN", "")
            .env_remove("SOMA_PROVIDER_DIR");
    }))
    .stderr(Stdio::null())
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
    assert!(!before_actions.contains(&"live_mcp_probe".to_owned()));
    assert!(!before_actions.contains(&"live_openapi_probe".to_owned()));

    fs::write(
        providers.join("live-ai-sdk.ts"),
        format!(
            "export default {};\nexport async function call(input) {{ return {{ ok: true, action: input.action }}; }}\n",
            provider_manifest("live-ai-sdk", "ai-sdk", "live_ts_probe")
        ),
    )?;
    fs::write(providers.join("live-wasm-provider.wasm"), wasm_provider()?)?;
    fs::write(
        providers.join("live-mcp-provider.json"),
        serde_json::to_vec_pretty(&json!({
            "schema_version": 1,
            "provider": {
                "name": "live-mcp-provider",
                "kind": "mcp",
                "enabled": true
            },
            "tools": [{
                "name": "live_mcp_probe",
                "description": "Live MCP provider action",
                "input_schema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {}
                },
                "cli": {
                    "enabled": true,
                    "command": "live-mcp-probe"
                },
                "rest": {
                    "enabled": true,
                    "method": "POST",
                    "path": "/v1/providers/live-mcp-probe"
                },
                "meta": {
                    "mcp": {
                        "upstream_tool": "soma",
                        "static_args": { "action": "status" }
                    }
                }
            }],
            "meta": {
                "mcp": {
                    "stdio": {
                        "command": env!("CARGO_BIN_EXE_soma"),
                        "args": ["mcp"],
                        "cwd": temp.path().display().to_string(),
                        "env": {
                            "SOMA_HOME": temp.path().display().to_string(),
                            "SOMA_API_URL": "",
                            "SOMA_API_KEY": "",
                            "RUST_LOG": "warn"
                        }
                    },
                    "timeout_ms": 10000
                }
            }
        }))?,
    )?;
    fs::write(
        providers.join("live-openapi-provider.json"),
        serde_json::to_vec_pretty(&json!({
            "schema_version": 1,
            "provider": {
                "name": "live-openapi-provider",
                "kind": "openapi",
                "enabled": true
            },
            "tools": [{
                "name": "live_openapi_probe",
                "description": "Live OpenAPI provider action",
                "input_schema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {}
                },
                "cli": {
                    "enabled": true,
                    "command": "live-openapi-probe"
                },
                "rest": {
                    "enabled": true,
                    "method": "POST",
                    "path": "/v1/providers/live-openapi-probe"
                },
                "meta": {
                    "openapi": {
                        "method": "POST",
                        "path": "/status"
                    }
                }
            }],
            "meta": {
                "openapi": {
                    "base_url": "http://127.0.0.1:9"
                }
            }
        }))?,
    )?;
    fs::write(
        providers.join("live-wasm-provider.json"),
        serde_json::to_vec_pretty(&json!({
            "schema_version": 1,
            "provider": {
                "name": "live-wasm-provider-docs",
                "kind": "static-rust",
                "enabled": false
            },
            "tools": []
        }))?,
    )?;
    println!("dropped_files={}", providers.display());

    let after = service.list_tools(Default::default()).await?;
    let after_actions = action_enum(&after.tools[0].input_schema);
    println!("after_actions={after_actions:?}");
    assert!(after_actions.contains(&"live_ts_probe".to_owned()));
    assert!(after_actions.contains(&"live_wasm_probe".to_owned()));
    assert!(after_actions.contains(&"live_mcp_probe".to_owned()));
    assert!(after_actions.contains(&"live_openapi_probe".to_owned()));

    let result = service
        .call_tool(
            CallToolRequestParams::new("soma").with_arguments(
                json!({"action": "live_wasm_probe"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await?;
    let structured = result
        .structured_content
        .expect("dynamic provider call should return structured content");
    println!("live_wasm_probe_result={structured}");
    assert_eq!(structured["ok"], true);
    assert_eq!(structured["action"], "live_wasm_probe");

    let cli_output = Command::new(env!("CARGO_BIN_EXE_soma"))
        .arg("live_wasm_probe")
        .current_dir(temp.path())
        .env("HOME", temp.path())
        .env("SOMA_HOME", temp.path())
        .env("SOMA_API_URL", "")
        .env("SOMA_API_KEY", "")
        .env("SOMA_MCP_TOKEN", "")
        .output()
        .await?;
    assert!(
        cli_output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&cli_output.stderr)
    );
    let cli_json: Value = serde_json::from_slice(&cli_output.stdout)?;
    assert_eq!(cli_json["action"], "live_wasm_probe");

    let port = unused_loopback_port()?;
    let _server = HttpServerGuard::spawn(temp.path(), port).await?;
    let rest_json = post_json(
        &format!("127.0.0.1:{port}"),
        "/v1/providers/live_wasm_probe",
        "{}",
    )
    .await?;
    assert_eq!(rest_json["action"], "live_wasm_probe");

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
            "cli": {
                "enabled": true,
                "command": action
            },
            "rest": {
                "enabled": true,
                "method": "POST",
                "path": format!("/v1/providers/{action}")
            }
        }]
    })
    .to_string()
}

fn unused_loopback_port() -> anyhow::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

struct HttpServerGuard {
    child: Child,
}

impl HttpServerGuard {
    async fn spawn(cwd: &std::path::Path, port: u16) -> anyhow::Result<Self> {
        let mut child = Command::new(env!("CARGO_BIN_EXE_soma-server"))
            .arg("serve")
            .current_dir(cwd)
            .env("HOME", cwd)
            .env("SOMA_HOME", cwd)
            .env("RUST_LOG", "warn")
            .env("SOMA_MCP_HOST", "127.0.0.1")
            .env("SOMA_MCP_PORT", port.to_string())
            .env("SOMA_MCP_NO_AUTH", "true")
            .env("SOMA_API_URL", "")
            .env("SOMA_API_KEY", "")
            .env("SOMA_MCP_TOKEN", "")
            .env_remove("SOMA_PROVIDER_DIR")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Err(error) = wait_for_health(port).await {
            let _ = child.start_kill();
            let mut stderr_text = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_string(&mut stderr_text).await;
            }
            return Err(anyhow::anyhow!("{error}; server stderr: {stderr_text}"));
        }
        Ok(Self { child })
    }
}

impl Drop for HttpServerGuard {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

async fn wait_for_health(port: u16) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let address = format!("127.0.0.1:{port}");
    loop {
        if Instant::now() > deadline {
            anyhow::bail!("HTTP server on {address} did not become healthy");
        }
        if get_raw(&address, "/health").await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn post_json(address: &str, path: &str, body: &str) -> anyhow::Result<Value> {
    let response = post_raw(address, path, body).await?;
    let body = response
        .split("\r\n\r\n")
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("missing HTTP body"))?;
    Ok(serde_json::from_str(body)?)
}

async fn post_raw(address: &str, path: &str, body: &str) -> anyhow::Result<String> {
    let mut stream = tokio::net::TcpStream::connect(address).await?;
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(request.as_bytes()).await?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    let response = String::from_utf8(response)?;
    if !response.starts_with("HTTP/1.1 200") && !response.starts_with("HTTP/1.0 200") {
        anyhow::bail!("HTTP request failed: {response}");
    }
    Ok(response)
}

async fn get_raw(address: &str, path: &str) -> anyhow::Result<String> {
    let mut stream = tokio::net::TcpStream::connect(address).await?;
    let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    let response = String::from_utf8(response)?;
    if !response.starts_with("HTTP/1.1 200") && !response.starts_with("HTTP/1.0 200") {
        anyhow::bail!("HTTP request failed: {response}");
    }
    Ok(response)
}

fn wasm_provider() -> anyhow::Result<Vec<u8>> {
    let mut bytes = wat::parse_str(
        r#"
(module
  (memory (export "memory") 1)
  (global $input_ptr (mut i32) (i32.const 1024))
  (global $output_ptr (mut i32) (i32.const 2048))
  (global $output_len (mut i32) (i32.const 38))
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
  (data (i32.const 2048) "{\"ok\":true,\"action\":\"live_wasm_probe\"}"))
"#,
    )?;
    append_provider_manifest(
        &mut bytes,
        provider_manifest("live-wasm-provider", "wasm", "live_wasm_probe").as_bytes(),
    );
    Ok(bytes)
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
