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
use rtemplate_contracts::providers::{ProviderCatalog, ProviderManifest};
use rtemplate_service::{
    provider_registry::Provider, providers::mcp::McpProvider, ProviderAuthMode, ProviderCall,
    ProviderPrincipal, ProviderRequestLimits, ProviderSurface,
};
use serde_json::{json, Map, Value};
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
                        "upstream_tool": "example",
                        "static_args": { "action": "echo" }
                    }
                }
            }],
            "meta": {
                "mcp": {
                    "stdio": {
                        "command": env!("CARGO_BIN_EXE_rtemplate"),
                        "args": ["mcp"],
                        "cwd": upstream.display().to_string()
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
            CallToolRequestParams::new("example").with_arguments(
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
    let catalog: ProviderCatalog = serde_json::from_value::<ProviderManifest>(json!({
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
                    "upstream_tool": "example",
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

    let output = McpProvider::new(catalog)
        .call(ProviderCall {
            provider: "upstream-http-mcp".to_owned(),
            action: "http_echo".to_owned(),
            params: json!({"message": "hello over http"}),
            principal: ProviderPrincipal {
                subject: "test".to_owned(),
                scopes: vec!["example:read".to_owned()],
            },
            auth_mode: ProviderAuthMode::Mounted,
            surface: ProviderSurface::Mcp,
            destructive_confirmed: false,
            limits: ProviderRequestLimits::default(),
            snapshot_id: "test-snapshot".to_owned(),
        })
        .await?;

    assert_eq!(output.value["echo"], "hello over http");
    Ok(())
}

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

fn unused_loopback_port() -> anyhow::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

struct HttpServerGuard {
    child: Child,
}

impl HttpServerGuard {
    async fn spawn(port: u16) -> anyhow::Result<Self> {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rtemplate-server"))
            .arg("serve")
            .env("RUST_LOG", "warn")
            .env("RTEMPLATE_MCP_HOST", "127.0.0.1")
            .env("RTEMPLATE_MCP_PORT", port.to_string())
            .env("RTEMPLATE_MCP_NO_AUTH", "true")
            .env("RTEMPLATE_API_URL", "")
            .env_remove("RTEMPLATE_API_KEY")
            .env_remove("RTEMPLATE_MCP_TOKEN")
            .env_remove("RTEMPLATE_PROVIDER_DIR")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Err(error) = wait_for_health(port).await {
            let _ = child.start_kill();
            return Err(error);
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
