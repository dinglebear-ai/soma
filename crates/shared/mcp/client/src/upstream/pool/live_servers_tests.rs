//! In-process `ServerHandler` fixtures (task lifecycle + echo/MRTR) used by
//! `live_tests.rs`. Split out to stay under the PATTERNS.md module size hard
//! limit.
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, CancelTaskParams, CreateTaskResult,
    DetailedTask, ElicitRequest, ElicitRequestParams, ElicitationSchema, GetPromptRequestParams,
    GetPromptResponse, GetPromptResult, GetTaskParams, GetTaskResult, InputRequest,
    InputRequiredResult, ListPromptsResult, ListResourcesResult, ListToolsResult,
    PaginatedRequestParams, ProtocolVersion, ReadResourceRequestParams, ReadResourceResponse,
    ReadResourceResult, Resource, ResourceContents, ServerCapabilities, ServerInfo, Task,
    TaskPayload, TaskStatus, Tool, UpdateTaskParams,
};
use rmcp::{ErrorData, RoleServer, ServerHandler};

#[derive(Default)]
pub(super) struct TaskFixtureState {
    update_completed: bool,
    cancel_requested: bool,
}

#[derive(Clone)]
pub(super) struct TaskServer {
    pub(super) state: Arc<Mutex<TaskFixtureState>>,
}

impl ServerHandler for TaskServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_tasks()
                .build(),
        )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: vec![
                Tool::new(
                    "start_update_task",
                    "creates an input-required task",
                    Arc::new(serde_json::Map::new()),
                ),
                Tool::new(
                    "start_cancel_task",
                    "creates a cancellable task",
                    Arc::new(serde_json::Map::new()),
                ),
            ],
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let task_id = match request.name.as_ref() {
            "start_update_task" => "native-update",
            "start_cancel_task" => "native-cancel",
            name => {
                return Err(ErrorData::invalid_params(
                    format!("unknown task fixture tool: {name}"),
                    None,
                ));
            }
        };
        Ok(CreateTaskResult::new(task_fixture(task_id, TaskStatus::Working)).into())
    }

    async fn get_task(
        &self,
        request: GetTaskParams,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, ErrorData> {
        let state = self.state.lock().expect("task fixture lock");
        let detailed = match request.task_id.as_str() {
            "native-update" if state.update_completed => DetailedTask::new(
                task_fixture("native-update", TaskStatus::Completed),
                TaskPayload::Completed {
                    result: serde_json::to_value(CallToolResult::structured(serde_json::json!({
                        "decision": "approved"
                    })))
                    .expect("tool result serializes")
                    .as_object()
                    .expect("tool result is object")
                    .clone(),
                },
            ),
            "native-update" => DetailedTask::new(
                task_fixture("native-update", TaskStatus::InputRequired),
                TaskPayload::InputRequired {
                    input_requests: approval_input_requests(),
                },
            ),
            "native-cancel" if state.cancel_requested => DetailedTask::new(
                task_fixture("native-cancel", TaskStatus::Cancelled),
                TaskPayload::Cancelled,
            ),
            "native-cancel" => DetailedTask::new(
                task_fixture("native-cancel", TaskStatus::Working),
                TaskPayload::Working,
            ),
            task_id => {
                return Err(ErrorData::invalid_params(
                    format!("unknown fixture task: {task_id}"),
                    None,
                ));
            }
        };
        Ok(GetTaskResult::new(detailed))
    }

    async fn update_task(
        &self,
        request: UpdateTaskParams,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        if request.task_id != "native-update" {
            return Err(ErrorData::invalid_params("unknown update task", None));
        }
        let decision = request
            .input_responses
            .get("approval")
            .and_then(|response| response.get("content"))
            .and_then(|content| content.get("decision"))
            .and_then(serde_json::Value::as_str);
        if decision != Some("approved") {
            return Err(ErrorData::invalid_params(
                "missing approved task input response",
                None,
            ));
        }
        self.state
            .lock()
            .expect("task fixture lock")
            .update_completed = true;
        Ok(())
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        if request.task_id != "native-cancel" {
            return Err(ErrorData::invalid_params("unknown cancel task", None));
        }
        self.state
            .lock()
            .expect("task fixture lock")
            .cancel_requested = true;
        Ok(())
    }
}

pub(super) fn task_fixture(task_id: &str, status: TaskStatus) -> Task {
    Task::new(
        task_id,
        status,
        "2026-07-27T00:00:00Z",
        "2026-07-27T00:00:00Z",
    )
    .with_poll_interval_ms(10)
}

#[derive(Clone)]
pub(super) struct EchoServer;

impl ServerHandler for EchoServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        .with_protocol_version(ProtocolVersion::V_2026_07_28)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: vec![
                Tool::new("echo", "echoes a message", Arc::new(serde_json::Map::new())),
                Tool::new(
                    "needs_input",
                    "requires one elicitation round",
                    Arc::new(serde_json::Map::new()),
                ),
            ],
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if request.name.as_ref() == "needs_input" {
            let Some(input_responses) = request.input_responses else {
                return Ok(mrtr_input_required().into());
            };
            if request.request_state.as_deref() != Some("mrtr-round-1") {
                return Err(ErrorData::invalid_params(
                    "missing or invalid MRTR requestState",
                    None,
                ));
            }
            if context.protocol_version() != Some(ProtocolVersion::V_2026_07_28)
                || context
                    .client_info()
                    .as_ref()
                    .map(|info| info.name.as_str())
                    != Some("downstream-agent")
                || context
                    .client_capabilities()
                    .and_then(|capabilities| capabilities.elicitation)
                    .and_then(|elicitation| elicitation.form)
                    .is_none()
            {
                return Err(ErrorData::invalid_params(
                    "downstream request identity or capabilities were not forwarded",
                    None,
                ));
            }
            let decision = input_responses
                .get("approval")
                .and_then(|response| response.get("content"))
                .and_then(|content| content.get("decision"))
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    ErrorData::invalid_params("missing approval input response", None)
                })?;
            return Ok(
                CallToolResult::structured(serde_json::json!({"decision": decision})).into(),
            );
        }

        let message = request
            .arguments
            .as_ref()
            .and_then(|args| args.get("message"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        Ok(CallToolResult::structured(serde_json::json!({"echo": message})).into())
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
    ) -> Result<ReadResourceResponse, ErrorData> {
        Ok(ReadResourceResult::new(vec![ResourceContents::text("hello", request.uri)]).into())
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
    ) -> Result<GetPromptResponse, ErrorData> {
        Ok(GetPromptResult::new(Vec::new()).into())
    }
}

pub(super) fn approval_input_requests() -> BTreeMap<String, InputRequest> {
    let request = ElicitRequest::new(ElicitRequestParams::FormElicitationParams {
        meta: None,
        message: "Approve this operation".to_owned(),
        requested_schema: ElicitationSchema::builder()
            .string_property("decision", |schema| schema)
            .build()
            .expect("valid elicitation schema"),
    });
    let mut input_requests = BTreeMap::new();
    input_requests.insert("approval".to_owned(), InputRequest::Elicitation(request));
    input_requests
}

pub(super) fn mrtr_input_required() -> InputRequiredResult {
    InputRequiredResult::new(
        Some(approval_input_requests()),
        Some("mrtr-round-1".to_owned()),
    )
}

pub(super) fn websocket_fixture_response(payload: &str) -> Option<serde_json::Value> {
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
