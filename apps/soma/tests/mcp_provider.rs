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
use soma_provider_adapters::gateway::UpstreamMcpProvider;
use soma_provider_core::{Provider as CoreProvider, ProviderCall as CoreProviderCall};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
};

#[tokio::test]
async fn hot_dropped_mcp_provider_proxies_upstream_tool_call() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let providers = temp.path().join("providers");
    let upstream = temp.path().join("upstream");
    fs::create_dir(&providers)?;
    fs::create_dir(&upstream)?;

    let service = stdio_client_in(temp.path()).await?;
    let before = service.list_tools(Default::default()).await?;
    assert!(!action_enum(&before.tools[0].input_schema).contains(&"upstream_echo".to_owned()));

    fs::write(
        providers.join("upstream-mcp.json"),
        serde_json::to_string_pretty(&json!({
            "schema_version": 1,
            "provider": {
                "name": "upstream-mcp",
                "kind": "mcp"
            },
            "tools": [{
                "name": "upstream_echo",
                "description": "Proxy echo through an upstream MCP server.",
                "input_schema": {
                    "type": "object",
                    "required": ["message"],
                    "additionalProperties": false,
                    "properties": {
                        "message": { "type": "string", "minLength": 1 }
                    }
                },
                "meta": {
                    "mcp": {
                        "upstream_tool": "soma",
                        "static_args": { "action": "echo" }
                    }
                }
            }],
            "meta": {
                "mcp": {
                    "stdio": {
                        "command": env!("CARGO_BIN_EXE_soma"),
                        "args": ["mcp"],
                        "cwd": upstream.display().to_string(),
                        "env": {
                            "SOMA_HOME": upstream.display().to_string(),
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

    let after = service.list_tools(Default::default()).await?;
    assert!(action_enum(&after.tools[0].input_schema).contains(&"upstream_echo".to_owned()));

    let result = service
        .call_tool(
            CallToolRequestParams::new("soma").with_arguments(
                json!({"action": "upstream_echo", "message": "hello"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await?;
    assert_eq!(result.structured_content.unwrap()["echo"], "hello");

    service.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn mcp_provider_infers_http_transport_from_url() -> anyhow::Result<()> {
    let port = unused_loopback_port()?;
    let _server = HttpServerGuard::spawn(port).await?;
    let catalog: soma_provider_core::ProviderCatalog =
        serde_json::from_value::<soma_provider_core::ProviderManifest>(json!({
            "schema_version": 1,
            "provider": {
                "name": "upstream-http-mcp",
                "kind": "mcp"
            },
            "tools": [{
                "name": "http_echo",
                "description": "Proxy echo through a streamable HTTP MCP server.",
                "input_schema": {
                    "type": "object",
                    "required": ["message"],
                    "additionalProperties": false,
                    "properties": {
                        "message": { "type": "string", "minLength": 1 }
                    }
                },
                "meta": {
                    "mcp": {
                        "upstream_tool": "soma",
                        "static_args": { "action": "echo" }
                    }
                }
            }],
            "meta": {
                "mcp": {
                    "url": format!("http://127.0.0.1:{port}/mcp"),
                    "timeout_ms": 10000
                }
            }
        }))?;

    let output = UpstreamMcpProvider::new(catalog)
        .call(CoreProviderCall {
            provider: "upstream-http-mcp".to_owned(),
            action: "http_echo".to_owned(),
            params: json!({"message": "hello over http"}),
            surface: soma_provider_core::ProviderSurface::Mcp,
            snapshot_id: "test-snapshot".to_owned(),
        })
        .await?;

    assert_eq!(output.value["echo"], "hello over http");
    Ok(())
}

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

fn unused_loopback_port() -> anyhow::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

struct HttpServerGuard {
    child: Child,
    _home: tempfile::TempDir,
}

impl HttpServerGuard {
    async fn spawn(port: u16) -> anyhow::Result<Self> {
        let home = tempfile::tempdir()?;
        let mut child = Command::new(env!("CARGO_BIN_EXE_soma"))
            .arg("serve")
            .env("HOME", home.path())
            .env("SOMA_HOME", home.path())
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
        if let Err(error) = wait_for_health(port, &mut child).await {
            let _ = child.start_kill();
            let stderr = child
                .wait_with_output()
                .await
                .map(|output| String::from_utf8_lossy(&output.stderr).trim().to_owned())
                .unwrap_or_else(|wait_error| format!("failed to capture stderr: {wait_error}"));
            return Err(anyhow::anyhow!("{error}; server stderr: {stderr}"));
        }
        Ok(Self { child, _home: home })
    }
}

impl Drop for HttpServerGuard {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

async fn wait_for_health(port: u16, child: &mut Child) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let address = format!("127.0.0.1:{port}");
    loop {
        if let Some(status) = child.try_wait()? {
            anyhow::bail!("HTTP MCP server on {address} exited before becoming healthy: {status}");
        }
        if Instant::now() > deadline {
            anyhow::bail!("HTTP MCP server on {address} did not become healthy");
        }
        if health_ok(&address).await.unwrap_or(false) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn health_ok(address: &str) -> anyhow::Result<bool> {
    let mut stream = tokio::net::TcpStream::connect(address).await?;
    stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    Ok(response.starts_with(b"HTTP/1.1 200") || response.starts_with(b"HTTP/1.0 200"))
}
