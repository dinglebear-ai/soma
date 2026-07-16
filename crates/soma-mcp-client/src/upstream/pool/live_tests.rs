use super::{
    bearer_token_from_env, capability_is_absent, normalize_bearer_value, websocket_authorization,
};
use crate::config::UpstreamConfig;
use crate::upstream::pool::{ToolCall, UpstreamPool};
use futures::{SinkExt, StreamExt};
use rmcp::model::{
    CallToolRequestParams, CallToolResult, GetPromptRequestParams, GetPromptResult,
    ListPromptsResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
    ReadResourceRequestParams, ReadResourceResult, Resource, ResourceContents, ServerCapabilities,
    ServerInfo, Tool,
};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use std::sync::Arc;
use tokio_tungstenite::tungstenite::Message;

#[test]
fn bearer_value_normalization_accepts_raw_or_prefixed_tokens() {
    assert_eq!(normalize_bearer_value("secret"), "secret");
    assert_eq!(normalize_bearer_value(" Bearer secret "), "secret");
}

#[test]
fn bearer_token_env_supports_plain_http_and_websocket_auth() {
    let var = "SOMA_MCP_CLIENT_TEST_BEARER";
    std::env::set_var(var, "Bearer secret");
    let config = UpstreamConfig {
        name: "bearer".to_owned(),
        bearer_token_env: Some(var.to_owned()),
        ..UpstreamConfig::default()
    };

    assert_eq!(bearer_token_from_env(&config).as_deref(), Some("secret"));
    assert_eq!(
        websocket_authorization(&config).as_deref(),
        Some("Bearer secret")
    );

    std::env::remove_var(var);
}

#[test]
fn capability_absence_matches_json_rpc_method_not_found() {
    assert!(capability_is_absent(
        "JSON-RPC error -32601: Method not found"
    ));
    assert!(capability_is_absent("method not found"));
    assert!(!capability_is_absent("connection refused"));
}

#[tokio::test]
async fn stdio_live_discovery_and_call_routes_echo() {
    let dir = tempfile::tempdir().expect("tempdir");
    let script = dir.path().join("stdio_mcp.py");
    std::fs::write(&script, STDIO_ECHO_SERVER).expect("write fixture");

    let pool = UpstreamPool::default();
    pool.register_config(UpstreamConfig {
        name: "py".to_owned(),
        command: Some(if cfg!(windows) {
            "python".to_owned()
        } else {
            "python3".to_owned()
        }),
        args: vec![script.to_string_lossy().to_string()],
        ..UpstreamConfig::default()
    })
    .expect("register upstream");

    let snapshots = pool.discover().await.expect("discover");
    let snapshot = snapshots
        .iter()
        .find(|snapshot| snapshot.name == "py")
        .expect("py snapshot");
    assert!(snapshot.health.is_routable(), "{:?}", snapshot.health);
    assert_eq!(snapshot.tools.len(), 1);
    assert_eq!(snapshot.resources.len(), 1);
    assert_eq!(snapshot.prompts.len(), 1);

    let result = pool
        .call_tool(ToolCall {
            upstream: "py".to_owned(),
            tool: "echo".to_owned(),
            params: serde_json::json!({"message": "smoke-0lnb"}),
        })
        .await
        .expect("tool call");

    assert_eq!(result, serde_json::json!({"echo": "smoke-0lnb"}));

    let resource = pool
        .read_resource("py", "test://one")
        .await
        .expect("resource read");
    assert_eq!(resource["contents"][0]["text"], "hello");

    let prompt = pool
        .get_prompt("py", "hello", None)
        .await
        .expect("prompt get");
    assert_eq!(prompt["messages"], serde_json::json!([]));
}

#[tokio::test]
async fn http_live_discovery_and_call_routes_echo() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind http smoke");
    let addr = listener.local_addr().expect("local addr");
    let service: StreamableHttpService<EchoServer, LocalSessionManager> =
        StreamableHttpService::new(
            || Ok(EchoServer),
            Default::default(),
            StreamableHttpServerConfig::default()
                .with_stateful_mode(false)
                .with_json_response(true),
        );
    let router = axum::Router::new().nest_service("/mcp", service);
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("http smoke server");
    });

    let pool = UpstreamPool::default();
    pool.register_config(UpstreamConfig {
        name: "http".to_owned(),
        url: Some(format!("http://{addr}/mcp")),
        ..UpstreamConfig::default()
    })
    .expect("register upstream");

    let snapshots = pool.discover().await.expect("discover");
    let snapshot = snapshots
        .iter()
        .find(|snapshot| snapshot.name == "http")
        .expect("http snapshot");
    assert!(snapshot.health.is_routable(), "{:?}", snapshot.health);
    assert_eq!(snapshot.tools[0].name, "echo");

    let result = pool
        .call_tool(ToolCall {
            upstream: "http".to_owned(),
            tool: "echo".to_owned(),
            params: serde_json::json!({"message": "http-smoke"}),
        })
        .await
        .expect("tool call");

    assert_eq!(result, serde_json::json!({"echo": "http-smoke"}));
    server.abort();
}

#[tokio::test]
async fn websocket_live_discovery_and_call_routes_echo() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind websocket smoke");
    let addr = listener.local_addr().expect("local addr");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept websocket");
        let mut socket = tokio_tungstenite::accept_async(stream)
            .await
            .expect("websocket handshake");
        while let Some(message) = socket.next().await {
            let Message::Text(text) = message.expect("websocket message") else {
                continue;
            };
            if let Some(response) = websocket_fixture_response(text.as_str()) {
                socket
                    .send(Message::Text(response.to_string().into()))
                    .await
                    .expect("send websocket response");
            }
        }
    });

    let pool = UpstreamPool::default();
    pool.register_config(UpstreamConfig {
        name: "ws".to_owned(),
        url: Some(format!("ws://{addr}/mcp")),
        ..UpstreamConfig::default()
    })
    .expect("register upstream");

    let snapshots = pool.discover().await.expect("discover");
    let snapshot = snapshots
        .iter()
        .find(|snapshot| snapshot.name == "ws")
        .expect("websocket snapshot");
    assert!(snapshot.health.is_routable(), "{:?}", snapshot.health);
    assert_eq!(snapshot.tools[0].name, "echo");

    let result = pool
        .call_tool(ToolCall {
            upstream: "ws".to_owned(),
            tool: "echo".to_owned(),
            params: serde_json::json!({"message": "websocket-smoke"}),
        })
        .await
        .expect("tool call");

    assert_eq!(result, serde_json::json!({"echo": "websocket-smoke"}));
    server.abort();
}

#[derive(Clone)]
struct EchoServer;

impl ServerHandler for EchoServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: vec![Tool::new(
                "echo",
                "echoes a message",
                Arc::new(serde_json::Map::new()),
            )],
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let message = request
            .arguments
            .as_ref()
            .and_then(|args| args.get("message"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        Ok(CallToolResult::structured(
            serde_json::json!({"echo": message}),
        ))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult {
            resources: vec![Resource::new("test://one", "one")],
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            "hello",
            request.uri,
        )]))
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        Ok(ListPromptsResult {
            prompts: vec![rmcp::model::Prompt::new(
                "hello",
                Some("hello prompt"),
                None,
            )],
            ..Default::default()
        })
    }

    async fn get_prompt(
        &self,
        _request: GetPromptRequestParams,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, ErrorData> {
        Ok(GetPromptResult::new(Vec::new()))
    }
}

fn websocket_fixture_response(payload: &str) -> Option<serde_json::Value> {
    let message: serde_json::Value = serde_json::from_str(payload).expect("json-rpc request");
    let id = message
        .get("id")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let method = message.get("method").and_then(serde_json::Value::as_str)?;
    let result = match method {
        "initialize" => serde_json::json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {"tools": {}, "resources": {}, "prompts": {}},
            "serverInfo": {"name": "ws-echo", "version": "0.0.0"}
        }),
        "notifications/initialized" => return None,
        "tools/list" => serde_json::json!({"tools": [{
            "name": "echo",
            "description": "echoes a message",
            "inputSchema": {"type": "object", "properties": {"message": {"type": "string"}}}
        }]}),
        "tools/call" => {
            let text = message["params"]["arguments"]["message"]
                .as_str()
                .unwrap_or_default();
            serde_json::json!({
                "content": [{"type": "text", "text": text}],
                "structuredContent": {"echo": text}
            })
        }
        "resources/list" => serde_json::json!({"resources": []}),
        "prompts/list" => serde_json::json!({"prompts": []}),
        _ => {
            return Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": "Method not found"}
            }));
        }
    };
    Some(serde_json::json!({"jsonrpc": "2.0", "id": id, "result": result}))
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
