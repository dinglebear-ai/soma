use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool, ToolAnnotations,
    },
    service::{ElicitationError, Peer, RequestContext},
    transport::stdio,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use soma_ops::OperationPlan;
use synapse_application::LegacyTool;
use tokio_util::sync::CancellationToken;

use crate::{ExecuteOptions, StandaloneError, StandaloneRuntime};

#[derive(Clone)]
pub struct SynapseMcpServer {
    runtime: Arc<StandaloneRuntime>,
}

const ELICIT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
struct ConfirmMutation {
    confirm: bool,
    understood: bool,
}

rmcp::elicit_safe!(ConfirmMutation);

impl SynapseMcpServer {
    pub fn new(runtime: Arc<StandaloneRuntime>) -> Self {
        Self { runtime }
    }
}

impl ServerHandler for SynapseMcpServer {
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: tool_definitions(self.runtime.as_ref()),
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        let result = match request.name.as_ref() {
            "synapse" => execute_canonical(self.runtime.as_ref(), arguments, &context.peer).await,
            "flux" => {
                execute_legacy(
                    self.runtime.as_ref(),
                    LegacyTool::Flux,
                    arguments,
                    &context.peer,
                )
                .await
            }
            "scout" => {
                execute_legacy(
                    self.runtime.as_ref(),
                    LegacyTool::Scout,
                    arguments,
                    &context.peer,
                )
                .await
            }
            name => Err(StandaloneError::UnknownOperation(format!(
                "unknown MCP tool: {name}"
            ))),
        };
        Ok(match result {
            Ok(value) => CallToolResult::structured(value),
            Err(error) => error_result(error),
        }
        .into())
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("synapse", env!("CARGO_PKG_VERSION")))
    }
}

pub async fn serve_stdio(runtime: Arc<StandaloneRuntime>) -> anyhow::Result<()> {
    let service = SynapseMcpServer::new(runtime).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

pub fn http_service(
    runtime: Arc<StandaloneRuntime>,
) -> StreamableHttpService<SynapseMcpServer, LocalSessionManager> {
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true);
    StreamableHttpService::new(
        move || Ok(SynapseMcpServer::new(Arc::clone(&runtime))),
        Default::default(),
        config,
    )
}

fn tool_definitions(runtime: &StandaloneRuntime) -> Vec<Tool> {
    vec![
        Tool::new_with_raw(
            Cow::Borrowed("synapse"),
            Some(Cow::Borrowed(
                "Execute one canonical Synapse infrastructure operation",
            )),
            Arc::new(canonical_schema(runtime)),
        )
        .with_annotations(ToolAnnotations::new().open_world(true)),
        legacy_tool(runtime, LegacyTool::Flux),
        legacy_tool(runtime, LegacyTool::Scout),
    ]
}

fn legacy_tool(runtime: &StandaloneRuntime, tool: LegacyTool) -> Tool {
    let schema = add_execution_fields(runtime.catalog().legacy_tool_schema(tool));
    let map = schema
        .as_object()
        .cloned()
        .expect("legacy tool schema is an object");
    Tool::new_with_raw(
        Cow::Borrowed(tool.as_str()),
        Some(Cow::Owned(format!(
            "Optional historical {} request alias returning canonical JSON",
            tool.as_str()
        ))),
        Arc::new(map),
    )
    .with_annotations(ToolAnnotations::new().open_world(true))
}

fn canonical_schema(runtime: &StandaloneRuntime) -> Map<String, Value> {
    let operations = runtime
        .catalog()
        .operations()
        .map(|operation| Value::String(operation.name().to_string()))
        .collect::<Vec<_>>();
    Map::from_iter([
        (
            "$schema".into(),
            Value::String("https://json-schema.org/draft/2020-12/schema".into()),
        ),
        ("type".into(), Value::String("object".into())),
        (
            "properties".into(),
            json!({
                "operation": {"type":"string", "enum": operations},
                "parameters": {"type":"object"},
                "confirmed": {"type":"boolean", "default":false},
                "idempotency_key": {"type":"string", "minLength":1, "maxLength":256},
                "actor": {"type":"string", "minLength":1, "maxLength":256}
            }),
        ),
        ("required".into(), json!(["operation", "parameters"])),
        ("additionalProperties".into(), Value::Bool(false)),
    ])
}

fn add_execution_fields(mut schema: Value) -> Value {
    let Some(branches) = schema.get_mut("oneOf").and_then(Value::as_array_mut) else {
        return schema;
    };
    for branch in branches {
        let Some(properties) = branch.get_mut("properties").and_then(Value::as_object_mut) else {
            continue;
        };
        properties.insert(
            "confirmed".into(),
            json!({"type":"boolean","default":false}),
        );
        properties.insert(
            "idempotency_key".into(),
            json!({"type":"string","minLength":1,"maxLength":256}),
        );
        properties.insert(
            "actor".into(),
            json!({"type":"string","minLength":1,"maxLength":256}),
        );
    }
    schema
}

async fn execute_canonical(
    runtime: &StandaloneRuntime,
    arguments: Value,
    peer: &Peer<RoleServer>,
) -> Result<Value, StandaloneError> {
    let object = arguments.as_object().ok_or_else(|| {
        StandaloneError::UnknownOperation("MCP arguments must be an object".into())
    })?;
    let operation = object
        .get("operation")
        .and_then(Value::as_str)
        .ok_or_else(|| StandaloneError::UnknownOperation("operation is required".into()))?;
    let parameters = object
        .get("parameters")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let mut options = surface_options(object);
    let cancellation = CancellationToken::new();
    match runtime
        .execute(operation, &parameters, &options, &cancellation)
        .await
    {
        Err(error @ StandaloneError::ConfirmationRequired(_)) if !options.confirmed => {
            elicit_confirmation(peer, error.plan().expect("confirmation carries plan")).await?;
            options.confirmed = true;
            runtime
                .execute(operation, &parameters, &options, &cancellation)
                .await
        }
        result => result,
    }
}

async fn execute_legacy(
    runtime: &StandaloneRuntime,
    tool: LegacyTool,
    arguments: Value,
    peer: &Peer<RoleServer>,
) -> Result<Value, StandaloneError> {
    let mut object = arguments.as_object().cloned().ok_or_else(|| {
        StandaloneError::UnknownOperation("MCP arguments must be an object".into())
    })?;
    let mut options = surface_options(&object);
    object.remove("confirmed");
    object.remove("idempotency_key");
    object.remove("actor");
    let input = Value::Object(object);
    let cancellation = CancellationToken::new();
    match runtime
        .execute_legacy(tool, &input, &options, &cancellation)
        .await
    {
        Err(error @ StandaloneError::ConfirmationRequired(_)) if !options.confirmed => {
            elicit_confirmation(peer, error.plan().expect("confirmation carries plan")).await?;
            options.confirmed = true;
            runtime
                .execute_legacy(tool, &input, &options, &cancellation)
                .await
        }
        result => result,
    }
}

async fn elicit_confirmation(
    peer: &Peer<RoleServer>,
    plan: &OperationPlan,
) -> Result<(), StandaloneError> {
    let target =
        serde_json::to_string(plan.target()).unwrap_or_else(|_| "<unserializable-target>".into());
    let message = format!(
        "Confirm {} for target {} using plan {}. This operation may change infrastructure state.",
        plan.operation(),
        target,
        plan.fingerprint().as_str()
    );
    let outcome =
        tokio::time::timeout(ELICIT_TIMEOUT, peer.elicit::<ConfirmMutation>(message)).await;
    match outcome {
        Err(_) => Err(confirmation_error("MCP mutation confirmation timed out")),
        Ok(Err(ElicitationError::UserDeclined)) => {
            Err(confirmation_error("MCP mutation confirmation was declined"))
        }
        Ok(Err(ElicitationError::UserCancelled)) => Err(confirmation_error(
            "MCP mutation confirmation was cancelled",
        )),
        Ok(Err(ElicitationError::CapabilityNotSupported)) => Err(confirmation_error(
            "MCP client does not support elicitation; provide confirmed=true only from a trusted caller",
        )),
        Ok(Err(error)) => Err(confirmation_error(&format!(
            "MCP mutation confirmation failed: {error}"
        ))),
        Ok(Ok(Some(answer))) if answer.confirm && answer.understood => Ok(()),
        Ok(Ok(_)) => Err(confirmation_error(
            "MCP mutation confirmation requires both confirm and understood",
        )),
    }
}

fn confirmation_error(message: &str) -> StandaloneError {
    StandaloneError::Other(anyhow::anyhow!(message.to_owned()))
}

fn surface_options(object: &Map<String, Value>) -> ExecuteOptions {
    ExecuteOptions {
        confirmed: object
            .get("confirmed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        idempotency_key: object
            .get("idempotency_key")
            .and_then(Value::as_str)
            .map(str::to_owned),
        actor: object
            .get("actor")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| Some("mcp".into())),
    }
}

fn error_result(error: StandaloneError) -> CallToolResult {
    let payload = if let Some(plan) = error.plan() {
        json!({
            "error": "confirmation_required",
            "message": error.to_string(),
            "plan": plan,
        })
    } else {
        json!({"error":"execution_failed", "message":error.to_string()})
    };
    CallToolResult::structured_error(payload)
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
