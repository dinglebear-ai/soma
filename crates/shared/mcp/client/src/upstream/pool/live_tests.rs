use crate::config::UpstreamConfig;

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

#[path = "live_servers_tests.rs"]
mod live_servers_tests;

use crate::upstream::{
    McpRequestOutcome, McpRoundTrip,
    pool::{ToolCall, UpstreamPool},
};
use axum::{
    Json,
    extract::State,
    http::{HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use futures::{SinkExt, StreamExt};
use live_servers_tests::{EchoServer, TaskFixtureState, TaskServer, websocket_fixture_response};
use rmcp::model::{
    ClientCapabilities, ElicitationCapability, FormElicitationCapability, Implementation,
    ProtocolVersion, RequestMetaObject,
};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn stdio_live_discovery_and_call_routes_echo() {
    let dir = tempfile::tempdir().expect("tempdir");
    let script = dir.path().join("stdio_mcp.py");
    std::fs::write(&script, STDIO_ECHO_SERVER).expect("write fixture");

    let pool = UpstreamPool::default();
    pool.register_config(UpstreamConfig {
        name: "py".to_owned(),
        command: Some(python_command()),
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

#[derive(Clone, Copy)]
enum DiscoveryFixtureMode {
    MethodNotFound,
    CompleteDiscovery,
}

#[derive(Clone)]
struct LifecycleFixture {
    mode: DiscoveryFixtureMode,
    discover_requests: Arc<AtomicUsize>,
    initialize_requests: Arc<AtomicUsize>,
    list_tools_requests: Arc<AtomicUsize>,
}

impl LifecycleFixture {
    fn new(mode: DiscoveryFixtureMode) -> Self {
        Self {
            mode,
            discover_requests: Arc::new(AtomicUsize::new(0)),
            initialize_requests: Arc::new(AtomicUsize::new(0)),
            list_tools_requests: Arc::new(AtomicUsize::new(0)),
        }
    }
}

async fn lifecycle_fixture_handler(
    State(fixture): State<LifecycleFixture>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let method = body
        .get("method")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let id = body.get("id").cloned().unwrap_or(serde_json::Value::Null);

    match method {
        "server/discover" => {
            fixture.discover_requests.fetch_add(1, Ordering::SeqCst);
            match fixture.mode {
                DiscoveryFixtureMode::MethodNotFound => Json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": -32601, "message": "server/discover method not found"}
                }))
                .into_response(),
                DiscoveryFixtureMode::CompleteDiscovery => Json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "resultType": "complete",
                        "supportedVersions": ["2026-07-28", "2025-11-25", "2025-06-18"],
                        "capabilities": {"tools": {}},
                        "ttlMs": 0,
                        "cacheScope": "private",
                        "_meta": {
                            "io.modelcontextprotocol/serverInfo": {
                                "name": "discovery-fixture",
                                "version": "1.0.0"
                            }
                        }
                    }
                }))
                .into_response(),
            }
        }
        "initialize" => {
            fixture.initialize_requests.fetch_add(1, Ordering::SeqCst);
            let mut response = Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "legacy-fixture", "version": "1.0.0"}
                }
            }))
            .into_response();
            response.headers_mut().insert(
                HeaderName::from_static("mcp-session-id"),
                HeaderValue::from_static("legacy-fixture-session"),
            );
            response
        }
        "notifications/initialized" => StatusCode::ACCEPTED.into_response(),
        "tools/list" => {
            fixture.list_tools_requests.fetch_add(1, Ordering::SeqCst);
            Json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": [{
                        "name": "legacy_echo",
                        "description": "legacy lifecycle fallback proof",
                        "inputSchema": {"type": "object"}
                    }]
                }
            }))
            .into_response()
        }
        other => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("unexpected MCP method: {other}"),
        )
            .into_response(),
    }
}

async fn run_lifecycle_fallback_fixture(mode: DiscoveryFixtureMode) -> LifecycleFixture {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind lifecycle fixture");
    let addr = listener.local_addr().expect("lifecycle fixture addr");
    let fixture = LifecycleFixture::new(mode);
    let router = axum::Router::new()
        .route("/mcp", axum::routing::post(lifecycle_fixture_handler))
        .with_state(fixture.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("lifecycle fixture server");
    });

    let pool = UpstreamPool::default();
    pool.register_config(UpstreamConfig {
        name: "legacy-http".to_owned(),
        url: Some(format!("http://{addr}/mcp")),
        proxy_resources: false,
        proxy_prompts: false,
        ..UpstreamConfig::default()
    })
    .expect("register lifecycle fixture");

    let snapshots = pool.discover().await.expect("discover legacy upstream");
    let snapshot = snapshots
        .iter()
        .find(|snapshot| snapshot.name == "legacy-http")
        .expect("legacy snapshot");
    assert!(snapshot.health.is_routable(), "{:?}", snapshot.health);
    assert_eq!(snapshot.tools.len(), 1);
    assert_eq!(snapshot.tools[0].name, "legacy_echo");

    server.abort();
    fixture
}

#[tokio::test]
async fn http_upstream_reconnects_with_initialize_after_method_not_found() {
    let fixture = run_lifecycle_fallback_fixture(DiscoveryFixtureMode::MethodNotFound).await;

    assert_eq!(fixture.discover_requests.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.initialize_requests.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.list_tools_requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn http_upstream_accepts_complete_discovery_without_legacy_initialize() {
    let fixture = run_lifecycle_fallback_fixture(DiscoveryFixtureMode::CompleteDiscovery).await;

    assert_eq!(fixture.discover_requests.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.initialize_requests.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.list_tools_requests.load(Ordering::SeqCst), 1);
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
                .with_legacy_session_mode(false)
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
async fn http_live_call_once_preserves_mrtr_state_input_responses_and_request_meta() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind MRTR smoke");
    let addr = listener.local_addr().expect("local addr");
    let service: StreamableHttpService<EchoServer, LocalSessionManager> =
        StreamableHttpService::new(
            || Ok(EchoServer),
            Default::default(),
            StreamableHttpServerConfig::default()
                .with_legacy_session_mode(false)
                .with_json_response(true),
        );
    let router = axum::Router::new().nest_service("/mcp", service);
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("MRTR smoke server");
    });

    let pool = UpstreamPool::default();
    pool.register_config(UpstreamConfig {
        name: "mrtr".to_owned(),
        url: Some(format!("http://{addr}/mcp")),
        ..UpstreamConfig::default()
    })
    .expect("register MRTR upstream");
    pool.discover().await.expect("discover MRTR upstream");

    let call = || ToolCall {
        upstream: "mrtr".to_owned(),
        tool: "needs_input".to_owned(),
        params: serde_json::json!({}),
    };
    let first = pool
        .call_tool_once(call(), McpRoundTrip::default())
        .await
        .expect("first MRTR round");
    let McpRequestOutcome::InputRequired(first) = first else {
        panic!("first MRTR round should require input");
    };
    assert_eq!(first["resultType"], "input_required");
    assert_eq!(first["requestState"], "mrtr-round-1");
    assert_eq!(
        first["inputRequests"]["approval"]["method"],
        "elicitation/create"
    );

    let mut input_responses = BTreeMap::new();
    input_responses.insert(
        "approval".to_owned(),
        serde_json::json!({
            "action": "accept",
            "content": {"decision": "approved"}
        }),
    );
    let forwarded_meta = RequestMetaObject::with_client_context(
        ProtocolVersion::V_2026_07_28,
        Implementation::new("downstream-agent", "9.9.9"),
        ClientCapabilities::builder()
            .enable_elicitation_with(
                ElicitationCapability::new().with_form(FormElicitationCapability::new()),
            )
            .build(),
    );
    let second = pool
        .call_tool_once(
            call(),
            McpRoundTrip {
                input_responses: Some(input_responses),
                request_state: Some("mrtr-round-1".to_owned()),
                request_meta: Some(
                    serde_json::to_value(forwarded_meta).expect("serialize request metadata"),
                ),
            },
        )
        .await
        .expect("second MRTR round");
    let McpRequestOutcome::Complete(second) = second else {
        panic!("second MRTR round should complete");
    };
    assert_eq!(second["resultType"], "complete");
    assert_eq!(second["structuredContent"]["decision"], "approved");

    server.abort();
}

#[tokio::test]
async fn http_live_tasks_forward_get_update_and_cancel() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind task smoke");
    let addr = listener.local_addr().expect("local addr");
    let state = Arc::new(Mutex::new(TaskFixtureState::default()));
    let service_state = Arc::clone(&state);
    let service: StreamableHttpService<TaskServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || {
                Ok(TaskServer {
                    state: Arc::clone(&service_state),
                })
            },
            Default::default(),
            StreamableHttpServerConfig::default()
                .with_legacy_session_mode(false)
                .with_json_response(true),
        );
    let router = axum::Router::new().nest_service("/mcp", service);
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("task smoke server");
    });

    let pool = UpstreamPool::default();
    pool.register_config(UpstreamConfig {
        name: "tasks".to_owned(),
        url: Some(format!("http://{addr}/mcp")),
        ..UpstreamConfig::default()
    })
    .expect("register task upstream");
    pool.discover().await.expect("discover task upstream");

    let update_task = pool
        .call_tool_once(
            ToolCall {
                upstream: "tasks".to_owned(),
                tool: "start_update_task".to_owned(),
                params: serde_json::json!({}),
            },
            McpRoundTrip::default(),
        )
        .await
        .expect("create update task");
    let McpRequestOutcome::Task(update_task) = update_task else {
        panic!("tool should return a task handle");
    };
    assert_eq!(update_task["taskId"], "native-update");

    let pending = pool
        .get_task("tasks", "native-update")
        .await
        .expect("poll input-required task");
    assert_eq!(pending["status"], "input_required");
    assert_eq!(
        pending["inputRequests"]["approval"]["method"],
        "elicitation/create"
    );

    let mut input_responses = BTreeMap::new();
    input_responses.insert(
        "approval".to_owned(),
        serde_json::json!({
            "action": "accept",
            "content": {"decision": "approved"}
        }),
    );
    pool.update_task("tasks", "native-update", input_responses)
        .await
        .expect("update task");
    let completed = pool
        .get_task("tasks", "native-update")
        .await
        .expect("poll completed task");
    assert_eq!(completed["status"], "completed");
    assert_eq!(
        completed["result"]["structuredContent"]["decision"],
        "approved"
    );

    let cancel_task = pool
        .call_tool_once(
            ToolCall {
                upstream: "tasks".to_owned(),
                tool: "start_cancel_task".to_owned(),
                params: serde_json::json!({}),
            },
            McpRoundTrip::default(),
        )
        .await
        .expect("create cancel task");
    let McpRequestOutcome::Task(cancel_task) = cancel_task else {
        panic!("tool should return a task handle");
    };
    assert_eq!(cancel_task["taskId"], "native-cancel");
    pool.cancel_task("tasks", "native-cancel")
        .await
        .expect("cancel task");
    let cancelled = pool
        .get_task("tasks", "native-cancel")
        .await
        .expect("poll cancelled task");
    assert_eq!(cancelled["status"], "cancelled");

    server.abort();
}

#[tokio::test]
async fn websocket_live_discovery_and_call_routes_echo() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind websocket smoke");
    let addr = listener.local_addr().expect("local addr");
    let server = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.expect("accept websocket");
            let mut socket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("websocket handshake");
            while let Some(message) = socket.next().await {
                let Ok(Message::Text(text)) = message else {
                    break;
                };
                if let Some(response) = websocket_fixture_response(text.as_str()) {
                    socket
                        .send(Message::Text(response.to_string().into()))
                        .await
                        .expect("send websocket response");
                }
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
