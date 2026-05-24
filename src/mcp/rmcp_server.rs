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

use lab_auth::AuthContext;
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
use serde_json::{json, Map, Value};

use crate::{
    actions::{action_names, is_known_action, required_scope_for_action, ValidationError},
    app::ScaffoldIntentValidationError,
    token_limit::MAX_RESPONSE_BYTES,
};

use crate::server::{AppState, AuthPolicy};

use super::{prompts, schemas::tool_definitions, tools::execute_tool};

const RESPONSE_OFFSET_PARAM: &str = "_response_offset";
const RESPONSE_PAGE_BYTES_PARAM: &str = "_response_page_bytes";
const DEFAULT_RESPONSE_PAGE_BYTES: usize = 16_000;
const MAX_RESPONSE_PAGE_BYTES: usize = 16_000;

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
            if !is_known_action(action_str) {
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
            if let Some(required_scope) = required_scope_for_action(action_str) {
                check_scope(auth, required_scope, action_str)?;
            }
        }

        let action: String = action_opt.unwrap_or_default();

        let mut arguments = request
            .arguments
            .map(Value::Object)
            .unwrap_or_else(|| Value::Object(Map::new()));
        strip_response_page_params(&mut arguments);
        let continuation_args = continuation_arguments(&arguments).cloned();

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
                    response_page,
                    &tool_name,
                    empty_action_as_none(&action),
                    continuation_args.as_ref(),
                )
            }
            Err(error) if crate::actions::is_validation_error(&error) => {
                tracing::warn!(
                    tool = %tool_name,
                    elapsed_ms = started.elapsed().as_millis(),
                    "MCP tool rejected invalid params"
                );
                tool_error_result(validation_error_payload(
                    &tool_name,
                    empty_action_as_none(&action),
                    &error,
                ))
            }
            Err(error) => {
                tracing::error!(
                    tool = %tool_name,
                    elapsed_ms = started.elapsed().as_millis(),
                    error = %error,
                    "MCP tool execution failed"
                );
                tool_error_result(execution_error_payload(
                    &tool_name,
                    empty_action_as_none(&action),
                    &error,
                ))
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResponsePageRequest {
    offset: usize,
    page_bytes: usize,
}

impl Default for ResponsePageRequest {
    fn default() -> Self {
        Self {
            offset: 0,
            page_bytes: DEFAULT_RESPONSE_PAGE_BYTES,
        }
    }
}

fn response_page_request(
    args: Option<&Map<String, Value>>,
) -> Result<ResponsePageRequest, ErrorData> {
    let Some(args) = args else {
        return Ok(ResponsePageRequest::default());
    };
    let offset = optional_usize_arg(args, RESPONSE_OFFSET_PARAM)?.unwrap_or(0);
    let page_bytes = optional_usize_arg(args, RESPONSE_PAGE_BYTES_PARAM)?
        .unwrap_or(DEFAULT_RESPONSE_PAGE_BYTES)
        .min(MAX_RESPONSE_PAGE_BYTES);
    if page_bytes == 0 {
        return Err(ErrorData::invalid_params(
            format!("{RESPONSE_PAGE_BYTES_PARAM} must be greater than zero"),
            Some(json!({
                "kind": "mcp_protocol_error",
                "schema_version": 1,
                "code": "invalid_response_page_bytes",
                "field": RESPONSE_PAGE_BYTES_PARAM,
                "retryable": true,
                "remediation": format!("Omit {RESPONSE_PAGE_BYTES_PARAM} or pass an integer from 1 to {MAX_RESPONSE_PAGE_BYTES}."),
            })),
        ));
    }
    Ok(ResponsePageRequest { offset, page_bytes })
}

fn optional_usize_arg(args: &Map<String, Value>, field: &str) -> Result<Option<usize>, ErrorData> {
    let Some(value) = args.get(field) else {
        return Ok(None);
    };
    let Some(value) = value.as_u64() else {
        return Err(ErrorData::invalid_params(
            format!("{field} must be an unsigned integer"),
            Some(json!({
                "kind": "mcp_protocol_error",
                "schema_version": 1,
                "code": "invalid_response_page_arg",
                "field": field,
                "retryable": true,
                "remediation": format!("Pass {field} as a non-negative integer."),
            })),
        ));
    };
    usize::try_from(value).map(Some).map_err(|_| {
        ErrorData::invalid_params(
            format!("{field} is too large"),
            Some(json!({
                "kind": "mcp_protocol_error",
                "schema_version": 1,
                "code": "response_page_arg_too_large",
                "field": field,
                "retryable": true,
                "remediation": format!("Pass a smaller {field} value."),
            })),
        )
    })
}

fn strip_response_page_params(arguments: &mut Value) {
    let Some(arguments) = arguments.as_object_mut() else {
        return;
    };
    arguments.remove(RESPONSE_OFFSET_PARAM);
    arguments.remove(RESPONSE_PAGE_BYTES_PARAM);
}

fn tool_result_from_json(
    value: Value,
    page_request: ResponsePageRequest,
    tool: &str,
    action: Option<&str>,
    continuation_args: Option<&Map<String, Value>>,
) -> Result<CallToolResult, ErrorData> {
    // Compact JSON (not pretty) recovers ~30-40% of the 40 KB token budget.
    let text = serde_json::to_string(&value)
        .map_err(|e| ErrorData::internal_error(format!("serialization error: {e}"), None))?;
    if text.len() <= MAX_RESPONSE_BYTES && page_request.offset == 0 {
        let mut result = CallToolResult::structured(value);
        result.content = vec![Content::text(text)];
        return Ok(result);
    }

    let payload = response_page_payload(&text, page_request, tool, action, continuation_args);
    let text = serde_json::to_string(&payload)
        .map_err(|e| ErrorData::internal_error(format!("serialization error: {e}"), None))?;
    let mut result = CallToolResult::structured(payload);
    result.content = vec![Content::text(text)];
    Ok(result)
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

fn response_page_payload(
    serialized: &str,
    page_request: ResponsePageRequest,
    tool: &str,
    action: Option<&str>,
    continuation_args: Option<&Map<String, Value>>,
) -> Value {
    let (offset, content, next_offset, has_more) =
        response_page_slice(serialized, page_request.offset, page_request.page_bytes);
    let continuation = has_more.then(|| {
        let arguments = continuation_arguments_with_page(
            continuation_args,
            action,
            next_offset,
            page_request.page_bytes,
        );
        json!({
            "tool": tool,
            "arguments": arguments,
            "note": "Call the same tool with the same original arguments plus these reserved continuation arguments.",
        })
    });

    json!({
        "kind": "mcp_response_page",
        "schema_version": 1,
        "code": "response_page",
        "message": "Tool response was returned as a scrollable serialized JSON page.",
        "truncated": false,
        "serialized_bytes": serialized.len(),
        "max_response_bytes": MAX_RESPONSE_BYTES,
        "content_format": "application/json-fragment",
        "content": content,
        "page": {
            "offset": offset,
            "page_bytes": page_request.page_bytes,
            "next_offset": next_offset,
            "has_more": has_more,
        },
        "continuation": continuation,
    })
}

fn continuation_arguments(arguments: &Value) -> Option<&Map<String, Value>> {
    arguments.as_object()
}

fn continuation_arguments_with_page(
    arguments: Option<&Map<String, Value>>,
    action: Option<&str>,
    next_offset: usize,
    page_bytes: usize,
) -> Value {
    let mut output = arguments.cloned().unwrap_or_default();
    if !output.contains_key("action") {
        output.insert(
            "action".to_owned(),
            action.map(Value::from).unwrap_or(Value::Null),
        );
    }
    output.insert(RESPONSE_OFFSET_PARAM.to_owned(), json!(next_offset));
    output.insert(RESPONSE_PAGE_BYTES_PARAM.to_owned(), json!(page_bytes));
    Value::Object(output)
}

fn response_page_slice(
    serialized: &str,
    requested_offset: usize,
    page_bytes: usize,
) -> (usize, &str, usize, bool) {
    let mut offset = requested_offset.min(serialized.len());
    while offset < serialized.len() && !serialized.is_char_boundary(offset) {
        offset += 1;
    }

    let mut end = offset.saturating_add(page_bytes).min(serialized.len());
    while end > offset && !serialized.is_char_boundary(end) {
        end -= 1;
    }

    (
        offset,
        &serialized[offset..end],
        end,
        end < serialized.len(),
    )
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

fn validation_error_payload(tool: &str, action: Option<&str>, error: &anyhow::Error) -> Value {
    if let Some(error) = error.downcast_ref::<ValidationError>() {
        return validation_error_payload_from_validation_error(tool, action, error);
    }
    if let Some(error) = error.downcast_ref::<ScaffoldIntentValidationError>() {
        let mut payload = base_tool_error_payload(
            "mcp_tool_error",
            error.code(),
            tool,
            action,
            error.to_string(),
            true,
            error.remediation(),
        );
        if let Some(field) = error.field() {
            payload["field"] = json!(field);
        }
        if let Some(expected_pattern) = error.expected_pattern() {
            payload["expected_pattern"] = json!(expected_pattern);
        }
        return payload;
    }
    base_tool_error_payload(
        "mcp_tool_error",
        "validation_error",
        tool,
        action,
        error.to_string(),
        true,
        "Correct the tool arguments and retry, or use action=help for examples.",
    )
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
    let mut payload = base_tool_error_payload(
        "mcp_tool_error",
        error.code(),
        tool,
        payload_action,
        error.to_string(),
        true,
        error.remediation(),
    );
    if let Some(field) = error.field() {
        payload["field"] = json!(field);
    }
    if let Some(bad_value) = error.bad_value() {
        payload["bad_value"] = json!(bad_value);
    }
    payload["available_actions"] = json!(action_names());
    payload
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

fn execution_error_payload(tool: &str, action: Option<&str>, error: &anyhow::Error) -> Value {
    let mut payload = base_tool_error_payload(
        "mcp_tool_error",
        "execution_error",
        tool,
        action,
        "Tool execution failed. Check server logs for details.",
        true,
        "Check service configuration and upstream availability, then retry. Use action=status or action=help for diagnostics.",
    );
    payload["reason_kind"] = json!(execution_error_kind(error));
    payload
}

fn execution_error_kind(error: &anyhow::Error) -> &'static str {
    let text = error.to_string().to_ascii_lowercase();
    if text.contains("timeout") || text.contains("timed out") {
        "timeout"
    } else if text.contains("rate limit") || text.contains("429") {
        "rate_limited"
    } else if text.contains("unauthorized")
        || text.contains("forbidden")
        || text.contains("401")
        || text.contains("403")
    {
        "auth_rejected"
    } else if text.contains("connection refused")
        || text.contains("connection reset")
        || text.contains("dns")
        || text.contains("unreachable")
        || text.contains("temporarily unavailable")
    {
        "upstream_unavailable"
    } else {
        "unknown"
    }
}

fn base_tool_error_payload(
    kind: &str,
    code: &str,
    tool: &str,
    action: Option<&str>,
    message: impl Into<String>,
    retryable: bool,
    remediation: impl Into<String>,
) -> Value {
    json!({
        "kind": kind,
        "schema_version": 1,
        "code": code,
        "tool": tool,
        "action": action,
        "message": message.into(),
        "retryable": retryable,
        "remediation": remediation.into(),
    })
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
                .get::<axum::http::request::Parts>()
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
    crate::actions::scopes_satisfy(token_scopes, required)
}

#[cfg(test)]
#[path = "rmcp_server_tests.rs"]
mod tests;
