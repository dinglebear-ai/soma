//! `ExampleRmcpServer` — the `ServerHandler` implementation.
//!
//! This is the adapter between the rmcp crate and your application. It:
//!   - Advertises tools, resources, and prompts to MCP clients
//!   - Enforces auth scopes on every call
//!   - Delegates business logic to `tools.rs` → `app.rs` → `example.rs`
//!
//! **Template**: rename `ExampleRmcpServer`. Update action metadata in
//! `src/actions.rs` to keep schemas, scope rules, and dispatch in sync.

use std::{borrow::Cow, sync::Arc, time::Instant};

use rmcp::{
    model::{
        CallToolRequestParams, CallToolResult, Content, GetPromptRequestParams, GetPromptResult,
        Implementation, ListPromptsResult, ListResourcesResult, ListToolsResult,
        PaginatedRequestParams, RawResource, ReadResourceRequestParams, ReadResourceResult,
        Resource, ResourceContents, ServerCapabilities, ServerInfo, Tool,
    },
    service::{Peer, RequestContext},
    ErrorData, RoleServer, ServerHandler,
};
#[cfg(feature = "auth")]
use rtemplate_auth::AuthContext;
#[cfg(not(feature = "auth"))]
struct AuthContext {
    sub: String,
    scopes: Vec<String>,
}
use serde_json::{json, Map, Value};

use rtemplate_contracts::{
    actions::{
        is_known_action_from, require_confirmation_if_destructive_from,
        required_scope_for_action_from, ValidationError,
    },
    errors::ServiceErrorKind,
    token_limit::MAX_RESPONSE_BYTES,
};

use rtemplate_runtime::server::{AppState, AuthPolicy};

use super::{
    prompts,
    response_paging::{
        response_page_request, strip_response_page_params, tool_result_from_cached_page,
        tool_result_from_json,
    },
    schemas::tool_definitions,
    tools::execute_tool,
};

// ── server ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ExampleRmcpServer {
    state: AppState,
}

pub fn rmcp_server(state: AppState) -> ExampleRmcpServer {
    ExampleRmcpServer { state }
}

impl ServerHandler for ExampleRmcpServer {
    // ── tools ─────────────────────────────────────────────────────────────────

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        require_auth_context(&self.state, &context)?;
        let tools = rmcp_tool_definitions()?;
        tracing::debug!(tool_count = tools.len(), "MCP tools listed");
        Ok(ListToolsResult {
            tools,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let tool_name = request.name.to_string();

        // Extract action before scope check so a missing action returns the
        // more useful "action is required" validation error, not DENY_SCOPE.
        let action_opt: Option<String> = request
            .arguments
            .as_ref()
            .and_then(|m| m.get("action"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);

        let response_page = response_page_request(request.arguments.as_ref())?;
        let auth = require_auth_context(&self.state, &context)?;
        if tool_name != "example" {
            return Err(unknown_tool_error(&tool_name));
        }
        if let Some(action_str) = action_opt.as_deref() {
            if !is_known_action_from(rtemplate_service::action_specs(), action_str) {
                tracing::warn!(
                    tool = %tool_name,
                    action = %action_str,
                    "MCP tool rejected unknown action"
                );
                return tool_error_result(unknown_action_payload(&tool_name, action_str));
            }
        }
        // Only scope-check when a known action is present; dispatch_example will
        // return the validation error for a missing action below.
        if let (Some(auth), Some(action_str)) = (auth, action_opt.as_deref()) {
            if let Some(required_scope) =
                required_scope_for_action_from(rtemplate_service::action_specs(), action_str)
            {
                check_scope(auth, required_scope, action_str)?;
            }
        }

        let action: String = action_opt.unwrap_or_default();
        if let Some(cursor) = response_page.cursor().map(str::to_owned) {
            return tool_result_from_cached_page(
                &self.state.response_pages,
                &cursor,
                response_page,
                &tool_name,
                empty_action_as_none(&action),
            );
        }

        let mut arguments = request
            .arguments
            .map(Value::Object)
            .unwrap_or_else(|| Value::Object(Map::new()));
        strip_response_page_params(&mut arguments);
        let continuation_args = arguments.as_object().cloned();

        // Destructive actions require an explicit "confirm": true. No-op for the
        // template's current (non-destructive) actions; gates any future one with
        // a structured validation error consistent with the dispatch error path.
        if let Err(tool_error) = require_confirmation_if_destructive_from(
            rtemplate_service::action_specs(),
            &action,
            &arguments,
        ) {
            return tool_error_result(
                tool_error.to_mcp_payload(&tool_name, empty_action_as_none(&action)),
            );
        }

        // Clone the peer so we can pass it to the tool dispatcher.
        // The peer is needed for elicitation (asking the client for user input).
        let peer: Peer<RoleServer> = context.peer.clone();

        let started = Instant::now();
        tracing::info!(tool = %tool_name, action = %action, "MCP tool execution started");

        match execute_tool(&self.state, &tool_name, arguments, &peer).await {
            Ok(result) => {
                tracing::info!(
                    tool = %tool_name,
                    elapsed_ms = started.elapsed().as_millis(),
                    "MCP tool execution completed"
                );
                tool_result_from_json(
                    result,
                    &self.state.response_pages,
                    response_page,
                    &tool_name,
                    empty_action_as_none(&action),
                    continuation_args.as_ref(),
                )
            }
            Err(error) => {
                let tool_error = rtemplate_service::classify_service_error(&error);
                if tool_error.kind == ServiceErrorKind::Validation {
                    tracing::warn!(
                        tool = %tool_name,
                        elapsed_ms = started.elapsed().as_millis(),
                        "MCP tool rejected invalid params"
                    );
                } else {
                    tracing::error!(
                        tool = %tool_name,
                        elapsed_ms = started.elapsed().as_millis(),
                        service_error_kind = %tool_error.kind.as_str(),
                        error = %error,
                        "MCP tool execution failed"
                    );
                }
                tool_error_result(
                    tool_error.to_mcp_payload(&tool_name, empty_action_as_none(&action)),
                )
            }
        }
    }

    // ── resources ─────────────────────────────────────────────────────────────

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        require_auth_context(&self.state, &context)?;
        Ok(ListResourcesResult {
            resources: vec![schema_resource()],
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        require_auth_context(&self.state, &context)?;
        if request.uri != SCHEMA_RESOURCE_URI {
            return Err(ErrorData::invalid_params(
                format!("unknown resource: {}", request.uri),
                None,
            ));
        }
        let schema = tool_definitions();
        let text = serde_json::to_string_pretty(&schema)
            .map_err(|e| ErrorData::internal_error(format!("serialization error: {e}"), None))?;
        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            text,
            SCHEMA_RESOURCE_URI,
        )
        .with_mime_type("application/json")]))
    }

    // ── prompts ───────────────────────────────────────────────────────────────

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        require_auth_context(&self.state, &context)?;
        Ok(prompts::list_prompts())
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, ErrorData> {
        require_auth_context(&self.state, &context)?;
        prompts::get_prompt(request).map_err(|e| ErrorData::invalid_params(e.to_string(), None))
    }

    // ── server info ───────────────────────────────────────────────────────────

    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        .with_server_info(Implementation::new(
            self.state.config.server_name.clone(),
            env!("CARGO_PKG_VERSION"),
        ))
    }
}

// ── resource definitions ──────────────────────────────────────────────────────

/// URI for the schema resource. **Template**: change `example` to your service name.
const SCHEMA_RESOURCE_URI: &str = "example://schema/mcp-tool";

fn schema_resource() -> Resource {
    Resource::new(
        RawResource::new(SCHEMA_RESOURCE_URI, "example tool schema")
            .with_description(
                "JSON schema for the example MCP tool and its action-based parameters",
            )
            .with_mime_type("application/json"),
        None,
    )
}

// ── tool definition conversion ────────────────────────────────────────────────

fn rmcp_tool_definitions() -> Result<Vec<Tool>, ErrorData> {
    tool_definitions()
        .iter()
        .cloned()
        .map(rmcp_tool_from_json)
        .collect()
}

fn rmcp_tool_from_json(value: Value) -> Result<Tool, ErrorData> {
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| ErrorData::internal_error("tool definition missing name", None))?;
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .map(|d| Cow::Owned(d.to_string()));
    let input_schema = value
        .get("inputSchema")
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| ErrorData::internal_error("tool definition missing inputSchema", None))?;
    Ok(Tool::new_with_raw(
        Cow::Owned(name.to_string()),
        description,
        Arc::new(input_schema),
    ))
}

fn tool_error_result(value: Value) -> Result<CallToolResult, ErrorData> {
    let text = serde_json::to_string(&value)
        .map_err(|e| ErrorData::internal_error(format!("serialization error: {e}"), None))?;
    let (payload, text) = if text.len() <= MAX_RESPONSE_BYTES {
        (value, text)
    } else {
        let payload = error_overflow_payload(&value, text.len());
        let text = serde_json::to_string(&payload)
            .map_err(|e| ErrorData::internal_error(format!("serialization error: {e}"), None))?;
        (payload, text)
    };
    let mut result = CallToolResult::structured_error(payload);
    result.content = vec![Content::text(text)];
    Ok(result)
}

fn error_overflow_payload(value: &Value, serialized_bytes: usize) -> Value {
    json!({
        "kind": "mcp_tool_error",
        "schema_version": 1,
        "code": "error_payload_too_large",
        "original_kind": value.get("kind").cloned().unwrap_or(Value::Null),
        "original_code": value.get("code").cloned().unwrap_or(Value::Null),
        "message": "Tool error payload exceeded the MCP response size limit. The original JSON was not returned to avoid invalid truncated JSON.",
        "retryable": true,
        "serialized_bytes": serialized_bytes,
        "max_response_bytes": MAX_RESPONSE_BYTES,
        "remediation": "Retry with narrower arguments. If this repeats, inspect server logs for the original error details.",
    })
}

fn validation_error_payload_from_validation_error(
    tool: &str,
    action: Option<&str>,
    error: &ValidationError,
) -> Value {
    let payload_action = action.or(match error {
        ValidationError::UnknownAction { action }
        | ValidationError::NotAvailableOverRest { action } => Some(action.as_str()),
        _ => None,
    });
    rtemplate_contracts::errors::ToolError::from_action_validation_with_actions(
        error,
        rtemplate_service::action_specs()
            .iter()
            .map(|spec| spec.name)
            .collect(),
    )
    .to_mcp_payload(tool, payload_action)
}

fn unknown_action_payload(tool: &str, action: &str) -> Value {
    validation_error_payload_from_validation_error(
        tool,
        Some(action),
        &ValidationError::UnknownAction {
            action: action.to_owned(),
        },
    )
}

fn empty_action_as_none(action: &str) -> Option<&str> {
    if action.is_empty() {
        None
    } else {
        Some(action)
    }
}

fn unknown_tool_error(tool_name: &str) -> ErrorData {
    ErrorData::invalid_params(
        format!("unknown tool: {tool_name}; available tools: example"),
        Some(json!({
            "kind": "mcp_protocol_error",
            "schema_version": 1,
            "code": "unknown_tool",
            "tool": tool_name,
            "available_tools": ["example"],
            "retryable": true,
            "remediation": "Call tools/list, then retry with one of the advertised tool names.",
        })),
    )
}

// ── auth helpers ──────────────────────────────────────────────────────────────

fn require_auth_context<'a>(
    state: &AppState,
    ctx: &'a RequestContext<RoleServer>,
) -> Result<Option<&'a AuthContext>, ErrorData> {
    match &state.auth_policy {
        AuthPolicy::LoopbackDev | AuthPolicy::TrustedGatewayUnscoped => Ok(None),
        AuthPolicy::Mounted { .. } => {
            let parts = ctx
                .extensions
                .get::<http::request::Parts>()
                .ok_or_else(|| {
                    tracing::error!(
                        "rmcp HTTP Parts extension absent — middleware ordering may be broken"
                    );
                    ErrorData::invalid_request(
                        "forbidden: missing http context",
                        Some(auth_protocol_error_payload(
                            "missing_http_context",
                            "MCP HTTP request context was unavailable for auth enforcement.",
                            "Check RMCP router mounting and middleware ordering. HTTP transports must preserve request Parts extensions before auth is enforced.",
                        )),
                    )
                })?;
            let auth = parts.extensions.get::<AuthContext>().ok_or_else(|| {
                tracing::warn!("AuthContext absent — AuthLayer may not be mounted");
                ErrorData::invalid_request(
                    "forbidden: missing auth context",
                    Some(auth_protocol_error_payload(
                        "missing_auth_context",
                        "MCP auth context was unavailable for this request.",
                        "Reconnect with a valid bearer token or OAuth session, and verify AuthLayer is mounted for the MCP route.",
                    )),
                )
            })?;
            Ok(Some(auth))
        }
    }
}

fn check_scope(auth: &AuthContext, required_scope: &str, action: &str) -> Result<(), ErrorData> {
    if scope_satisfied(&auth.scopes, required_scope) {
        return Ok(());
    }
    tracing::warn!(
        subject = %auth.sub,
        action = %action,
        required_scope = %required_scope,
        "MCP tool denied: insufficient scope"
    );
    Err(ErrorData::invalid_request(
        format!("forbidden: requires scope: {required_scope}"),
        Some(json!({
            "kind": "mcp_auth_error",
            "schema_version": 1,
            "code": "insufficient_scope",
            "action": action,
            "required_scope": required_scope,
            "granted_scopes": auth.scopes,
            "message": "Authenticated caller does not have the required MCP scope for this action.",
            "retryable": false,
            "remediation": "Request a token/session with the required scope, or choose an action allowed by the current token scopes.",
        })),
    ))
}

fn auth_protocol_error_payload(
    code: &str,
    message: impl Into<String>,
    remediation: impl Into<String>,
) -> Value {
    json!({
        "kind": "mcp_auth_error",
        "schema_version": 1,
        "code": code,
        "message": message.into(),
        "retryable": false,
        "remediation": remediation.into(),
    })
}

fn scope_satisfied(token_scopes: &[String], required: &str) -> bool {
    rtemplate_contracts::actions::scopes_satisfy(token_scopes, required)
}

#[cfg(test)]
#[path = "rmcp_server_tests.rs"]
mod tests;
