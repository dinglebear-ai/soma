//! `SomaRmcpServer` — the `ServerHandler` implementation.
//!
//! This is the adapter between the rmcp crate and your application. It:
//!   - Advertises tools, resources, and prompts to MCP clients
//!   - Enforces auth scopes on every call
//!   - Delegates business logic to `tools.rs` → `app.rs` → `soma.rs`
//!
//! **Customize**: rename `SomaRmcpServer`. Update action metadata in
//! `src/actions.rs` to keep schemas, scope rules, and dispatch in sync.

use std::{borrow::Cow, sync::Arc, time::Instant};

use rmcp::{
    model::{
        CallToolRequestParams, CallToolResult, ContentBlock, GetPromptRequestParams,
        GetPromptResult, Implementation, ListPromptsResult, ListResourceTemplatesResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams,
        ReadResourceResult, Resource, ResourceContents, ResourceTemplate, ServerCapabilities,
        ServerInfo, Tool,
    },
    service::{Peer, RequestContext},
    ErrorData, RoleServer, ServerHandler,
};
use rmcp_traces::{TraceSummary, TraceTrust};
use serde_json::{json, Map, Value};

use soma_contracts::{
    errors::ServiceErrorKind, providers::ProviderResource, token_limit::MAX_RESPONSE_BYTES,
};
use soma_mcp_server::response_paging::{
    response_page_request, strip_response_page_params, tool_result_from_cached_page,
    tool_result_from_json, ResponsePagingOptions,
};

use soma_runtime::server::AppState;
use soma_service::{ProviderError, ResourceReadOutput};

use super::{
    conformance, gateway_proxy, prompts,
    rmcp_auth::{
        gateway_oauth_subject, protected_route_scope, protected_scope_allows_service,
        provider_auth_mode, provider_principal, require_auth_context,
    },
    schemas::tool_definitions_for_catalogs as tool_definitions,
    tools::execute_tool,
    ACTION_DISCRIMINATOR_FIELD,
};

macro_rules! trace_summary_event {
    ($level:ident, $trace_summary:expr, $message:literal, $($field:tt)*) => {
        tracing::$level!(
            $($field)*
            trace_id_prefix = ?$trace_summary.trace_id_prefix(),
            span_id_prefix = ?$trace_summary.span_id_prefix(),
            trace_sampled = ?$trace_summary.sampled(),
            trace_trust = ?$trace_summary.trust(),
            has_tracestate = $trace_summary.has_tracestate(),
            baggage_member_count = $trace_summary.baggage_member_count(),
            sensitive_baggage_member_count = $trace_summary.sensitive_baggage_member_count(),
            trace_invalid_count = $trace_summary.invalid_count(),
            trace_invalid_reasons = ?$trace_summary.invalid_reasons(),
            $message
        );
    };
}

// ── server ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SomaRmcpServer {
    state: AppState,
}

pub fn rmcp_server(state: AppState) -> SomaRmcpServer {
    SomaRmcpServer { state }
}

impl ServerHandler for SomaRmcpServer {
    // ── tools ─────────────────────────────────────────────────────────────────

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let auth = require_auth_context(&self.state, &context)?;
        let route_scope = protected_route_scope(&context);
        let soma_allowed = protected_scope_allows_service(route_scope, "soma");
        refresh_file_providers(&self.state)?;
        let mut tools = if soma_allowed {
            rmcp_tool_definitions(&self.state)?
        } else {
            Vec::new()
        };
        let gateway_subject = gateway_oauth_subject(auth);
        tools.extend(
            gateway_proxy::list_tools_for_subject_and_scope(
                &self.state.gateway,
                Some(gateway_subject.as_ref()),
                route_scope,
            )
            .await?,
        );
        if soma_allowed && self.state.config.conformance_fixtures {
            tools.extend(conformance::tool_definitions());
        }
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

        let response_page = match response_page_request(request.arguments.as_ref()) {
            Ok(response_page) => response_page,
            Err(error) => {
                tracing::warn!("MCP tool rejected response paging params");
                return Err(error);
            }
        };
        let auth = match require_auth_context(&self.state, &context) {
            Ok(auth) => auth,
            Err(error) => {
                tracing::warn!("MCP tool rejected auth context");
                return Err(error);
            }
        };
        let trace_summary = trace_summary_from_context(&context);
        let route_scope = protected_route_scope(&context);
        let soma_allowed = protected_scope_allows_service(route_scope, "soma");
        if soma_allowed && self.state.config.conformance_fixtures {
            if let Some(result) = conformance::call_tool(&tool_name) {
                return Ok(result);
            }
        }
        if tool_name != "soma" {
            let gateway_subject = gateway_oauth_subject(auth);
            if let Some(result) = gateway_proxy::call_tool_for_subject_and_scope(
                &self.state.gateway,
                &tool_name,
                request.arguments.clone(),
                Some(gateway_subject.as_ref()),
                route_scope,
            )
            .await
            {
                return Ok(result);
            }
            trace_summary_event!(
                warn,
                trace_summary,
                "MCP tool rejected unknown tool",
                tool = %tool_name,
                action = action_opt.as_deref().unwrap_or_default(),
            );
            return Err(unknown_tool_error(&tool_name));
        }
        if !soma_allowed {
            return Err(unknown_tool_error(&tool_name));
        }
        let action: String = action_opt.unwrap_or_default();
        if let Some(cursor) = response_page.cursor().map(str::to_owned) {
            trace_summary_event!(
                info,
                trace_summary,
                "MCP tool returned cached response page",
                tool = %tool_name,
                action = empty_action_as_none(&action).unwrap_or_default(),
            );
            return tool_result_from_cached_page(
                &self.state.response_pages,
                &cursor,
                response_page,
                response_paging_options(),
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

        // Clone the peer so we can pass it to the tool dispatcher.
        // The peer is needed for elicitation (asking the client for user input).
        let peer: Peer<RoleServer> = context.peer.clone();
        let principal = provider_principal(auth);
        let auth_mode = provider_auth_mode(&self.state.auth_policy);

        let started = Instant::now();
        trace_summary_event!(
            info,
            trace_summary,
            "MCP tool execution started",
            tool = %tool_name,
            action = %action,
        );

        match execute_tool(
            &self.state,
            &tool_name,
            arguments,
            &peer,
            principal,
            auth_mode,
        )
        .await
        {
            Ok(result) => {
                trace_summary_event!(
                    info,
                    trace_summary,
                    "MCP tool execution completed",
                    tool = %tool_name,
                    action = %action,
                    elapsed_ms = started.elapsed().as_millis(),
                );
                tool_result_from_json(
                    result,
                    &self.state.response_pages,
                    response_page,
                    response_paging_options(),
                    &tool_name,
                    empty_action_as_none(&action),
                    continuation_args.as_ref(),
                )
            }
            Err(error) => {
                let tool_error = soma_service::classify_service_error(&error);
                if tool_error.kind == ServiceErrorKind::Validation {
                    trace_summary_event!(
                        warn,
                        trace_summary,
                        "MCP tool rejected invalid params",
                        tool = %tool_name,
                        action = %action,
                        elapsed_ms = started.elapsed().as_millis(),
                    );
                } else {
                    trace_summary_event!(
                        error,
                        trace_summary,
                        "MCP tool execution failed",
                        tool = %tool_name,
                        action = %action,
                        elapsed_ms = started.elapsed().as_millis(),
                        service_error_kind = %tool_error.kind.as_str(),
                        error = %error,
                    );
                }
                tool_error_result(
                    provider_error_payload(&error, &tool_name, empty_action_as_none(&action))
                        .unwrap_or_else(|| {
                            tool_error.to_mcp_payload(&tool_name, empty_action_as_none(&action))
                        }),
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
        let auth = require_auth_context(&self.state, &context)?;
        let route_scope = protected_route_scope(&context);
        refresh_file_providers(&self.state)?;
        let soma_allowed = protected_scope_allows_service(route_scope, "soma");
        let mut resources = if soma_allowed {
            let snapshot = self.state.provider_registry.snapshot();
            let mut resources = vec![schema_resource()];
            resources.extend(
                snapshot
                    .exact_resources()
                    .map(rmcp_resource_from_catalog_resource),
            );
            resources
        } else {
            Vec::new()
        };
        let gateway_subject = gateway_oauth_subject(auth);
        resources.extend(
            gateway_proxy::list_resources_for_subject_and_scope(
                &self.state.gateway,
                Some(gateway_subject.as_ref()),
                route_scope,
            )
            .await?,
        );
        if soma_allowed && self.state.config.conformance_fixtures {
            resources.extend(conformance::resources());
        }
        Ok(ListResourcesResult {
            resources,
            ..Default::default()
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        require_auth_context(&self.state, &context)?;
        refresh_file_providers(&self.state)?;
        let snapshot = self.state.provider_registry.snapshot();
        let mut resource_templates: Vec<ResourceTemplate> = snapshot
            .dynamic_resource_templates()
            .iter()
            .map(|(_, template)| {
                let mut built =
                    ResourceTemplate::new(template.uri_template(), template.name.clone())
                        .with_description(template.description.clone());
                if let Some(mime_type) = &template.mime_type {
                    built = built.with_mime_type(mime_type.clone());
                }
                built
            })
            .collect();
        if self.state.config.conformance_fixtures {
            resource_templates.extend(conformance::resource_templates());
        }
        Ok(ListResourceTemplatesResult {
            resource_templates,
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        let auth = require_auth_context(&self.state, &context)?;
        let route_scope = protected_route_scope(&context);
        let soma_allowed = protected_scope_allows_service(route_scope, "soma");
        if soma_allowed && self.state.config.conformance_fixtures {
            if let Some(result) = conformance::read_resource(&request.uri) {
                return Ok(result);
            }
        }
        let gateway_subject = gateway_oauth_subject(auth);
        if let Some(result) = gateway_proxy::read_resource_for_subject_and_scope(
            &self.state.gateway,
            &request.uri,
            Some(gateway_subject.as_ref()),
            route_scope,
        )
        .await?
        {
            return Ok(result);
        }
        if !soma_allowed {
            return Err(ErrorData::invalid_params(
                format!("unknown resource: {}", request.uri),
                None,
            ));
        }
        refresh_file_providers(&self.state)?;
        if request.uri == SCHEMA_RESOURCE_URI {
            let schema = tool_definitions_for_state(&self.state);
            let text = serde_json::to_string_pretty(&schema).map_err(|e| {
                ErrorData::internal_error(format!("serialization error: {e}"), None)
            })?;
            return Ok(ReadResourceResult::new(vec![ResourceContents::text(
                text,
                SCHEMA_RESOURCE_URI,
            )
            .with_mime_type("application/json")]));
        }

        let principal = provider_principal(auth);
        let auth_mode = provider_auth_mode(&self.state.auth_policy);
        let output = self
            .state
            .provider_registry
            .read_resource(&request.uri, &principal, auth_mode)
            .await
            .map_err(|error| resource_read_error(&request.uri, &error))?;
        Ok(ReadResourceResult::new(vec![
            resource_contents_from_output(&request.uri, output),
        ]))
    }

    // ── prompts ───────────────────────────────────────────────────────────────

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, ErrorData> {
        let auth = require_auth_context(&self.state, &context)?;
        let route_scope = protected_route_scope(&context);
        let soma_allowed = protected_scope_allows_service(route_scope, "soma");
        let mut result = if soma_allowed {
            prompts::list_prompts()
        } else {
            ListPromptsResult::default()
        };
        refresh_file_providers(&self.state)?;
        if soma_allowed {
            let snapshot = self.state.provider_registry.snapshot();
            result
                .prompts
                .extend(prompts::provider_prompts(&snapshot.catalogs));
        }
        let gateway_subject = gateway_oauth_subject(auth);
        result.prompts.extend(
            gateway_proxy::list_prompts_for_subject_and_scope(
                &self.state.gateway,
                Some(gateway_subject.as_ref()),
                route_scope,
            )
            .await?,
        );
        if soma_allowed && self.state.config.conformance_fixtures {
            result.prompts.extend(conformance::prompts());
        }
        Ok(result)
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, ErrorData> {
        let auth = require_auth_context(&self.state, &context)?;
        let route_scope = protected_route_scope(&context);
        let soma_allowed = protected_scope_allows_service(route_scope, "soma");
        if soma_allowed && self.state.config.conformance_fixtures {
            if let Some(result) = conformance::get_prompt(request.clone()) {
                return Ok(result);
            }
        }
        let gateway_subject = gateway_oauth_subject(auth);
        if let Some(result) = gateway_proxy::get_prompt_for_subject_and_scope(
            &self.state.gateway,
            &request.name,
            request.arguments.clone(),
            Some(gateway_subject.as_ref()),
            route_scope,
        )
        .await?
        {
            return Ok(result);
        }
        if !soma_allowed {
            return Err(ErrorData::invalid_params(
                format!("unknown prompt: {}", request.name),
                None,
            ));
        }
        refresh_file_providers(&self.state)?;
        let snapshot = self.state.provider_registry.snapshot();
        match prompts::get_provider_prompt(
            &snapshot.catalogs,
            &request,
            provider_auth_mode(&self.state.auth_policy),
            &provider_principal(auth),
        ) {
            prompts::ProviderPromptLookup::Found(result) => return Ok(result),
            prompts::ProviderPromptLookup::ScopeDenied { required_scope } => {
                return Err(ErrorData::invalid_request(
                    format!(
                        "forbidden: prompt `{}` requires scope `{required_scope}`",
                        request.name
                    ),
                    None,
                ));
            }
            prompts::ProviderPromptLookup::NotFound => {}
        }
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
        .with_instructions(SERVER_INSTRUCTIONS)
    }
}

fn response_paging_options() -> ResponsePagingOptions {
    ResponsePagingOptions {
        max_response_bytes: MAX_RESPONSE_BYTES,
        action_discriminator_field: ACTION_DISCRIMINATOR_FIELD,
    }
}

const SERVER_INSTRUCTIONS: &str = "\
Soma is a batteries-included RMCP runtime for shipping provider-backed MCP servers. \
It exposes one action-dispatched `soma` tool plus first-class MCP prompt and resource surfaces. \
Homepage: https://soma.dinglebear.ai. Repository: https://github.com/jmagar/soma. \
Node package: soma-rmcp. Binary: soma. \
Config home: ~/.soma or SOMA_HOME. License: MIT. Author: dinglebear.ai. \
Use drop-in providers to add tools, prompts, and resources without rewriting transport, auth, \
schema, paging, config, Docker, plugin, or release plumbing. A new server comes online by adding \
provider files under providers/tools, providers/prompts, providers/resources, or another configured \
provider source. Clients should discover `soma://schema/mcp-tool` before invoking actions, call \
`status` or `help` to inspect available providers, and send JSON action arguments matching the \
advertised schema. Responses are structured JSON; large payloads may be paged through Soma's \
resource paging flow.";

// ── resource definitions ──────────────────────────────────────────────────────

/// URI for the schema resource. **Customize**: change `soma` to your service name.
const SCHEMA_RESOURCE_URI: &str = "soma://schema/mcp-tool";

fn schema_resource() -> Resource {
    Resource::new(SCHEMA_RESOURCE_URI, "soma tool schema")
        .with_description("JSON schema for the Soma MCP tool and its action-based parameters")
        .with_mime_type("application/json")
}

fn rmcp_resource_from_catalog_resource(resource: &ProviderResource) -> Resource {
    let mut built = Resource::new(resource.uri_template.clone(), resource.name.clone())
        .with_description(resource.description.clone());
    if let Some(mime_type) = &resource.mime_type {
        built = built.with_mime_type(mime_type.clone());
    }
    built
}

fn resource_contents_from_output(uri: &str, output: ResourceReadOutput) -> ResourceContents {
    match output {
        ResourceReadOutput::Text { text, mime_type } => {
            let mut contents = ResourceContents::text(text, uri);
            if let Some(mime_type) = mime_type {
                contents = contents.with_mime_type(mime_type);
            }
            contents
        }
        ResourceReadOutput::Blob {
            blob_base64,
            mime_type,
        } => {
            let mut contents = ResourceContents::blob(blob_base64, uri);
            if let Some(mime_type) = mime_type {
                contents = contents.with_mime_type(mime_type);
            }
            contents
        }
    }
}

/// Maps a `ProviderRegistry::read_resource` failure to the protocol-level
/// `ErrorData` MCP `resources/read` expects — there is no structured
/// tool-result-style "isError" channel for resource reads the way
/// `call_tool` has, so every failure kind maps to `ErrorData`.
fn resource_read_error(uri: &str, error: &ProviderError) -> ErrorData {
    match error.code.as_ref() {
        "unknown_resource" => ErrorData::invalid_params(format!("unknown resource: {uri}"), None),
        "insufficient_scope" => {
            ErrorData::invalid_request(format!("forbidden: {}", error.message), None)
        }
        _ => ErrorData::internal_error(error.message.to_string(), None),
    }
}

// ── tool definition conversion ────────────────────────────────────────────────

fn rmcp_tool_definitions(state: &AppState) -> Result<Vec<Tool>, ErrorData> {
    tool_definitions_for_state(state)
        .into_iter()
        .map(rmcp_tool_from_json)
        .collect()
}

fn refresh_file_providers(state: &AppState) -> Result<(), ErrorData> {
    state
        .provider_registry
        .refresh_file_providers()
        .map(|_| ())
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))
}

fn tool_definitions_for_state(state: &AppState) -> Vec<Value> {
    let snapshot = state.provider_registry.snapshot();
    tool_definitions(&snapshot.catalogs)
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
    let mut tool = Tool::new_with_raw(
        Cow::Owned(name.to_string()),
        description,
        Arc::new(input_schema),
    );
    if let Some(output_schema) = value.get("outputSchema") {
        let output_schema = output_schema.as_object().cloned().ok_or_else(|| {
            ErrorData::internal_error("tool outputSchema must be an object", None)
        })?;
        tool = tool.with_raw_output_schema(Arc::new(output_schema));
    }
    Ok(tool)
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
    result.content = vec![ContentBlock::text(text)];
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

fn provider_error_payload(
    error: &anyhow::Error,
    tool: &str,
    fallback_action: Option<&str>,
) -> Option<Value> {
    let error = error.downcast_ref::<soma_service::ProviderError>()?;
    Some(json!({
        "kind": "mcp_tool_error",
        "schema_version": error.schema_version,
        "code": error.code,
        "tool": tool,
        "provider": error.provider,
        "action": error.action.as_deref().or(fallback_action),
        "message": error.message,
        "retryable": error.retryable,
        "remediation": error.remediation,
        "provider_error_kind": error.kind,
    }))
}

fn empty_action_as_none(action: &str) -> Option<&str> {
    if action.is_empty() {
        None
    } else {
        Some(action)
    }
}

fn trace_summary_from_context(context: &RequestContext<RoleServer>) -> TraceSummary {
    trace_summary_from_meta(&context.meta)
}

fn trace_summary_from_meta(meta: &rmcp::model::Meta) -> TraceSummary {
    TraceSummary::from_meta(meta, TraceTrust::Untrusted)
}

fn unknown_tool_error(tool_name: &str) -> ErrorData {
    ErrorData::invalid_params(
        format!("unknown tool: {tool_name}; available tools: soma"),
        Some(json!({
            "kind": "mcp_protocol_error",
            "schema_version": 1,
            "code": "unknown_tool",
            "tool": tool_name,
            "available_tools": ["soma"],
            "retryable": true,
            "remediation": "Call tools/list, then retry with one of the advertised tool names.",
        })),
    )
}

#[cfg(test)]
#[path = "rmcp_server_tests.rs"]
mod tests;
