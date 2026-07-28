use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CancelTaskParams, ClientCapabilities, ClientInfo,
    ElicitationCapability, FormElicitationCapability, GetPromptRequestParams, GetTaskParams,
    Implementation, ProtocolVersion, ReadResourceRequestParams, ResourceContents, TaskStatus,
    UpdateTaskParams,
};
use rmcp::{
    service::{ClientLifecycleMode, ClientServiceExt},
    ServiceExt,
};
use serde_json::{json, Map, Value};
use soma_application::{
    ExecutionContext, GatewayExecuteRequest, GatewayMcpOutcome, GatewayMcpRoundTrip, GatewayPort,
    GatewayPromptRoute, GatewayReloadRequest, GatewayResourceRoute, GatewayRouteScope,
    GatewayToolRoute, PortError,
};
use soma_test_support::{tracing_test_lock, SharedBuf};

use crate::{rmcp_server, testing::loopback_state_with_gateway};

#[derive(Default)]
struct RecordingGateway {
    task_updated: AtomicBool,
    task_cancelled: AtomicBool,
    danger_called: AtomicBool,
}

#[async_trait]
impl GatewayPort for RecordingGateway {
    async fn status(&self, _context: &ExecutionContext) -> Result<Value, PortError> {
        Ok(json!({}))
    }

    async fn reload(
        &self,
        _request: GatewayReloadRequest,
        _context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        Ok(json!({}))
    }

    async fn execute(
        &self,
        _request: GatewayExecuteRequest,
        _context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        Ok(json!({}))
    }

    async fn list_mcp_tools(
        &self,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Vec<GatewayToolRoute>, PortError> {
        Ok([
            ("echo", "echoes a message", false),
            ("fail", "always fails", false),
            ("task", "returns an asynchronous task", false),
            ("danger", "requires destructive confirmation", true),
        ]
        .into_iter()
        .map(|(name, description, destructive)| GatewayToolRoute {
            name: name.to_owned(),
            description: Some(description.to_owned()),
            input_schema: Some(json!({"type": "object"})),
            output_schema: None,
            destructive,
        })
        .collect())
    }

    async fn call_mcp_tool(
        &self,
        name: &str,
        params: Value,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError> {
        if name == "fail" {
            return Err(PortError::new("upstream_failed", "synthetic failure"));
        }
        if name == "danger" {
            self.danger_called.store(true, Ordering::SeqCst);
            return Ok(Some(json!({"danger": true})));
        }
        Ok((name == "echo").then(|| json!({"echo": params["message"]})))
    }

    async fn call_mcp_tool_once(
        &self,
        name: &str,
        params: Value,
        _round_trip: GatewayMcpRoundTrip,
        scope: Option<&GatewayRouteScope>,
        context: &ExecutionContext,
    ) -> Result<Option<GatewayMcpOutcome>, PortError> {
        if name == "task" {
            return Ok(Some(GatewayMcpOutcome::Task(json!({
                "resultType": "task",
                "taskId": "soma-task-test",
                "status": "working",
                "statusMessage": "started",
                "createdAt": "2026-07-27T00:00:00Z",
                "lastUpdatedAt": "2026-07-27T00:00:00Z",
                "ttlMs": null,
                "pollIntervalMs": 10
            }))));
        }
        self.call_mcp_tool(name, params, scope, context)
            .await
            .map(|value| {
                value.map(|structured_content| {
                    GatewayMcpOutcome::Complete(json!({
                        "resultType": "complete",
                        "content": [],
                        "structuredContent": structured_content
                    }))
                })
            })
    }

    async fn get_mcp_task(
        &self,
        task_id: &str,
        _context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        if task_id != "soma-task-test" {
            return Err(PortError::new("task_missing", "unknown synthetic task"));
        }
        let common = json!({
            "resultType": "complete",
            "taskId": task_id,
            "createdAt": "2026-07-27T00:00:00Z",
            "lastUpdatedAt": "2026-07-27T00:00:00Z",
            "ttlMs": null,
            "pollIntervalMs": 10
        });
        let mut task = common.as_object().expect("task object").clone();
        if self.task_cancelled.load(Ordering::SeqCst) {
            task.insert("status".to_owned(), json!("cancelled"));
        } else if self.task_updated.load(Ordering::SeqCst) {
            task.insert("status".to_owned(), json!("completed"));
            task.insert(
                "result".to_owned(),
                json!({
                    "resultType": "complete",
                    "content": [],
                    "structuredContent": {"decision": "approved"}
                }),
            );
        } else {
            task.insert("status".to_owned(), json!("input_required"));
            task.insert(
                "inputRequests".to_owned(),
                json!({
                    "approval": {
                        "method": "elicitation/create",
                        "params": {
                            "mode": "form",
                            "message": "Approve the task",
                            "requestedSchema": {
                                "type": "object",
                                "properties": {
                                    "decision": {"type": "string"}
                                }
                            }
                        }
                    }
                }),
            );
        }
        Ok(Value::Object(task))
    }

    async fn update_mcp_task(
        &self,
        task_id: &str,
        input_responses: std::collections::BTreeMap<String, Value>,
        _context: &ExecutionContext,
    ) -> Result<(), PortError> {
        if task_id != "soma-task-test" {
            return Err(PortError::new("task_missing", "unknown synthetic task"));
        }
        let approved = input_responses
            .get("approval")
            .and_then(|response| response.get("content"))
            .and_then(|content| content.get("decision"))
            .and_then(Value::as_str)
            == Some("approved");
        if !approved {
            return Err(PortError::new(
                "invalid_task_input",
                "approval input response is required",
            ));
        }
        self.task_updated.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn cancel_mcp_task(
        &self,
        task_id: &str,
        _context: &ExecutionContext,
    ) -> Result<(), PortError> {
        if task_id != "soma-task-test" {
            return Err(PortError::new("task_missing", "unknown synthetic task"));
        }
        self.task_cancelled.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn list_mcp_resources(
        &self,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Vec<GatewayResourceRoute>, PortError> {
        Ok(vec![GatewayResourceRoute {
            uri: "mcp-gateway://upstream/mock/test%3A%2F%2Fone".to_owned(),
            native_uri: "test://one".to_owned(),
            name: Some("one".to_owned()),
        }])
    }

    async fn read_mcp_resource(
        &self,
        uri: &str,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError> {
        Ok(Some(
            json!({"contents": [{"uri": uri, "mimeType": "text/plain", "text": "hello"}]}),
        ))
    }

    async fn list_mcp_prompts(
        &self,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Vec<GatewayPromptRoute>, PortError> {
        Ok(vec![GatewayPromptRoute {
            name: "hello".to_owned(),
            description: Some("hello prompt".to_owned()),
        }])
    }

    async fn get_mcp_prompt(
        &self,
        name: &str,
        _arguments: Option<Map<String, Value>>,
        _scope: Option<&GatewayRouteScope>,
        _context: &ExecutionContext,
    ) -> Result<Option<Value>, PortError> {
        Ok((name == "hello").then(|| json!({"messages": []})))
    }
}

#[allow(clippy::await_holding_lock)]
#[tokio::test(flavor = "current_thread")]
async fn mcp_server_exposes_application_gateway_tools_resources_and_prompts() -> anyhow::Result<()>
{
    let _lock = tracing_test_lock();
    let buf = SharedBuf::new();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(buf.writer())
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::INFO)
        .finish();
    let guard = tracing::subscriber::set_default(subscriber);
    let gateway = std::sync::Arc::new(RecordingGateway::default());
    let state = loopback_state_with_gateway(gateway.clone());
    let (server_transport, client_transport) = tokio::io::duplex(16 * 1024);
    let server_handle = tokio::spawn(async move {
        rmcp_server(state)
            .serve(server_transport)
            .await?
            .waiting()
            .await?;
        anyhow::Ok(())
    });
    let client = ClientInfo::new(
        ClientCapabilities::builder()
            .enable_tasks()
            .enable_elicitation_with(
                ElicitationCapability::new().with_form(FormElicitationCapability::new()),
            )
            .build(),
        Implementation::new("soma-task-test-client", env!("CARGO_PKG_VERSION")),
    )
    .with_protocol_version(ProtocolVersion::V_2026_07_28)
    .serve_with_lifecycle(
        client_transport,
        ClientLifecycleMode::Discover {
            preferred_versions: vec![ProtocolVersion::V_2026_07_28],
        },
    )
    .await?;

    let tools = client.list_tools(Default::default()).await?;
    assert!(tools.tools.iter().any(|tool| tool.name == "soma"));
    assert!(tools.tools.iter().any(|tool| tool.name == "echo"));
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
    let failed = client.call_tool(CallToolRequestParams::new("fail")).await?;
    assert_eq!(failed.is_error, Some(true));

    let confirmation = client
        .call_tool_once(CallToolRequestParams::new("danger"))
        .await?;
    let CallToolResponse::InputRequired(confirmation) = confirmation else {
        panic!("destructive gateway tool should require form input");
    };
    assert!(!gateway.danger_called.load(Ordering::SeqCst));
    let confirmation_json = serde_json::to_value(&confirmation)?;
    assert_eq!(
        confirmation_json["inputRequests"]["destructive_confirmation"]["method"],
        "elicitation/create"
    );

    let mut confirmed_request = CallToolRequestParams::new("danger");
    confirmed_request.input_responses = Some(std::collections::BTreeMap::from([(
        "destructive_confirmation".to_owned(),
        json!({
            "action": "accept",
            "content": {"confirm": true}
        }),
    )]));
    let confirmed = client.call_tool_once(confirmed_request).await?;
    let CallToolResponse::Complete(confirmed) = confirmed else {
        panic!("confirmed destructive gateway tool should complete");
    };
    assert_eq!(confirmed.structured_content, Some(json!({"danger": true})));
    assert!(gateway.danger_called.load(Ordering::SeqCst));

    let task = client
        .call_tool_once(CallToolRequestParams::new("task"))
        .await?;
    let CallToolResponse::Task(task) = task else {
        panic!("gateway task tool should return a task handle");
    };
    let task_id = task.task.task_id.clone();
    assert_eq!(task_id, "soma-task-test");

    let pending = client.get_task(GetTaskParams::new(task_id.clone())).await?;
    assert_eq!(pending.task.status(), TaskStatus::InputRequired);
    let pending_json = serde_json::to_value(&pending)?;
    assert_eq!(
        pending_json["inputRequests"]["approval"]["method"],
        "elicitation/create"
    );

    let mut input_responses = std::collections::BTreeMap::new();
    input_responses.insert(
        "approval".to_owned(),
        json!({
            "action": "accept",
            "content": {"decision": "approved"}
        }),
    );
    client
        .update_task(UpdateTaskParams::new(task_id.clone(), input_responses))
        .await?;
    let completed = client.get_task(GetTaskParams::new(task_id.clone())).await?;
    assert_eq!(completed.task.status(), TaskStatus::Completed);
    let completed_json = serde_json::to_value(&completed)?;
    assert_eq!(
        completed_json["result"]["structuredContent"]["decision"],
        "approved"
    );

    client
        .cancel_task(CancelTaskParams::new(task_id.clone()))
        .await?;
    let cancelled = client.get_task(GetTaskParams::new(task_id)).await?;
    assert_eq!(cancelled.task.status(), TaskStatus::Cancelled);

    let resources = client.list_resources(Default::default()).await?;
    let uri = resources
        .resources
        .iter()
        .find(|resource| resource.uri.starts_with("mcp-gateway://"))
        .expect("gateway resource")
        .uri
        .clone();
    let resource = client
        .read_resource(ReadResourceRequestParams::new(uri))
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
    drop(guard);

    let logs = buf.contents();
    assert!(
        logs.contains("MCP gateway tool execution completed"),
        "logs were: {logs}"
    );
    assert!(logs.contains("tool=echo"), "logs were: {logs}");
    assert!(
        logs.contains("MCP gateway tool execution failed"),
        "logs were: {logs}"
    );
    assert!(logs.contains("tool=fail"), "logs were: {logs}");
    assert_eq!(
        logs.matches("MCP gateway tool execution completed").count(),
        2,
        "only the echo and confirmed destructive calls should be logged as completed: {logs}"
    );
    Ok(())
}
