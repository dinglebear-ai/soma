use rmcp::model::{
    CallToolRequestParams, GetPromptRequestParams, ReadResourceRequestParams, ResourceContents,
};
use rmcp::ServiceExt;
use serde_json::json;
use soma_gateway::config::{GatewayConfig, UpstreamConfig};
use soma_runtime::server::gateway_product_state_from_config;

use crate::rmcp_server;
use crate::testing::loopback_state;

fn python_command() -> String {
    std::env::var("SOMA_PYTHON_COMMAND")
        .ok()
        .and_then(|value| bare_command_name(&value))
        .unwrap_or_else(default_python_command)
}

fn bare_command_name(value: &str) -> Option<String> {
    value
        .trim()
        .trim_matches('"')
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

fn default_python_command() -> String {
    if cfg!(windows) {
        "python".to_owned()
    } else {
        "python3".to_owned()
    }
}

#[tokio::test]
async fn mcp_server_exposes_live_gateway_tools_resources_and_prompts() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let script = dir.path().join("stdio_mcp.py");
    std::fs::write(&script, STDIO_ECHO_SERVER)?;

    let mut state = loopback_state();
    state.gateway = gateway_product_state_from_config(GatewayConfig {
        upstream: vec![UpstreamConfig {
            name: "py".to_owned(),
            command: Some(python_command()),
            args: vec![script.to_string_lossy().to_string()],
            ..UpstreamConfig::default()
        }],
        ..GatewayConfig::default()
    })?;

    let (server_transport, client_transport) = tokio::io::duplex(16 * 1024);
    let server_handle = tokio::spawn(async move {
        rmcp_server(state)
            .serve(server_transport)
            .await?
            .waiting()
            .await?;
        anyhow::Ok(())
    });
    let client = ().serve(client_transport).await?;

    let tools = client.list_tools(Default::default()).await?;
    let tool_names: Vec<&str> = tools.tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert!(tool_names.contains(&"soma"));
    assert!(tool_names.contains(&"echo"));

    let echo = client
        .call_tool(
            CallToolRequestParams::new("echo").with_arguments(
                json!({"message": "through-soma"})
                    .as_object()
                    .expect("object")
                    .clone(),
            ),
        )
        .await?;
    assert_eq!(
        echo.structured_content,
        Some(json!({"echo": "through-soma"}))
    );

    let resources = client.list_resources(Default::default()).await?;
    let route_uri = resources
        .resources
        .iter()
        .map(|resource| resource.uri.as_str())
        .find(|uri| uri.starts_with("mcp-gateway://upstream/py/"))
        .expect("gateway resource route")
        .to_owned();
    let resource = client
        .read_resource(ReadResourceRequestParams::new(route_uri))
        .await?;
    match &resource.contents[0] {
        ResourceContents::TextResourceContents { text, .. } => assert_eq!(text, "hello"),
        other => panic!("unexpected resource contents: {other:?}"),
    }

    let prompts = client.list_prompts(Default::default()).await?;
    assert!(prompts.prompts.iter().any(|prompt| prompt.name == "hello"));
    let prompt = client
        .get_prompt(GetPromptRequestParams::new("hello"))
        .await?;
    assert!(prompt.messages.is_empty());

    client.cancel().await?;
    server_handle.await??;
    Ok(())
}

const STDIO_ECHO_SERVER: &str = r#"
import json
import sys

def send(id, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": id, "result": result}) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    if not line.strip():
        continue
    msg = json.loads(line)
    method = msg.get("method")
    id = msg.get("id")
    if method == "initialize":
        send(id, {
            "protocolVersion": "2025-06-18",
            "capabilities": {"tools": {}, "resources": {}, "prompts": {}},
            "serverInfo": {"name": "stdio-echo", "version": "0.0.0"}
        })
    elif method == "notifications/initialized":
        pass
    elif method == "tools/list":
        send(id, {"tools": [{
            "name": "echo",
            "description": "echoes a message",
            "inputSchema": {"type": "object", "properties": {"message": {"type": "string"}}}
        }]})
    elif method == "tools/call":
        args = msg.get("params", {}).get("arguments", {})
        send(id, {
            "content": [{"type": "text", "text": args.get("message", "")}],
            "structuredContent": {"echo": args.get("message", "")}
        })
    elif method == "resources/list":
        send(id, {"resources": [{"uri": "test://one", "name": "one"}]})
    elif method == "resources/read":
        uri = msg.get("params", {}).get("uri", "test://one")
        send(id, {"contents": [{"uri": uri, "mimeType": "text/plain", "text": "hello"}]})
    elif method == "prompts/list":
        send(id, {"prompts": [{"name": "hello", "description": "hello prompt"}]})
    elif method == "prompts/get":
        send(id, {"messages": []})
    else:
        sys.stdout.write(json.dumps({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32601, "message": "Method not found"}
        }) + "\n")
        sys.stdout.flush()
"#;
