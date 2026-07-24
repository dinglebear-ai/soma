//! `SomaRmcpServer` — the `ServerHandler` implementation.
//!
//! This is the adapter between the rmcp crate and your application. It:
//!   - Advertises tools, resources, and prompts to MCP clients
//!   - Enforces auth scopes on every call
//!   - Delegates business logic to `tools.rs` → `app.rs` → `soma-client`
//!
//! **Customize**: rename `SomaRmcpServer`. Update action metadata in
//! `src/actions.rs` to keep schemas, scope rules, and dispatch in sync.

use std::time::Instant;

use rmcp::{
    model::{
        CallToolRequestParams, CallToolResult, GetPromptRequestParams, GetPromptResult,
        Implementation, ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult,
        ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult,
        Resource, ResourceContents, ResourceTemplate, ServerCapabilities, ServerInfo, Tool,
    },
    service::{Peer, RequestContext},
    ErrorData, RoleServer, ServerHandler,
};
use rmcp_traces::TraceTrust;
use serde_json::{Map, Value};

use soma_application::{ApplicationError, ExecutionContext, ReadResourceRequest, ResourceContent};
use soma_domain::{token_limit::MAX_RESPONSE_BYTES, TraceContext};
use soma_mcp_server::{
    conformance,
    response_paging::{
        response_page_request, strip_response_page_params, tool_result_from_cached_page,
        tool_result_from_json, ResponsePagingOptions,
    },
};
use soma_provider_core::ProviderResource;

use super::{
    gateway_proxy, prompts,
    protocol_errors::{application_error_payload, tool_error_result, unknown_tool_error},
    rmcp_auth::{
        principal, protected_route_scope, protected_scope_allows_service, require_auth_context,
        AuthContext,
    },
    schemas::tool_definitions_for_catalogs as tool_definitions,
    state::McpState,
    tools::execute_tool,
    trace_resolution, ACTION_DISCRIMINATOR_FIELD,
};

macro_rules! trace_summary_event {
    ($level:ident, $trace_resolution:expr, $trace_context_conflict:expr, $message:literal, $($field:tt)*) => {
        tracing::$level!(
            $($field)*
            trace_id_prefix = ?$trace_resolution.summary.trace_id_prefix(),
            span_id_prefix = ?$trace_resolution.summary.span_id_prefix(),
            trace_sampled = ?$trace_resolution.summary.sampled(),
            trace_trust = ?$trace_resolution.summary.trust(),
            has_tracestate = $trace_resolution.summary.has_tracestate(),
            baggage_member_count = $trace_resolution.summary.baggage_member_count(),
            sensitive_baggage_member_count = $trace_resolution.summary.sensitive_baggage_member_count(),
            trace_invalid_count = $trace_resolution.summary.invalid_count(),
            trace_invalid_reasons = ?$trace_resolution.summary.invalid_reasons(),
            http_trace_headers_present = $trace_resolution.http_trace_headers_present,
            trace_context_conflict = $trace_context_conflict,
            $message
        );
    };
}

// ── server ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SomaRmcpServer {
    state: McpState,
}

pub fn rmcp_server(state: McpState) -> SomaRmcpServer {
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
        let execution_context = execution_context(&self.state, &context, auth);
        let soma_allowed = protected_scope_allows_service(route_scope, "soma");
        refresh_file_providers(&self.state)?;
        let mut tools = if soma_allowed {
            rmcp_tool_definitions(&self.state)?
        } else {
            Vec::new()
        };
        tools.extend(
            gateway_proxy::list_tools_for_subject_and_scope(
                self.state.application(),
                route_scope,
                &execution_context,
            )
            .await?,
        );
        if soma_allowed && self.state.config().conformance_fixtures {
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
        let trace_resolution = trace_resolution_for_call(&self.state, &context);
        let trace_context_conflict = trace_resolution.http_trace_headers_present
            && trace_resolution::meta_has_any_trace_key(&context.meta);
        let route_scope = protected_route_scope(&context);
        let execution_context =
            execution_context_with_trace(&self.state, auth, trace_resolution.trace_context.clone());
        let soma_allowed = protected_scope_allows_service(route_scope, "soma");
        if soma_allowed && self.state.config().conformance_fixtures {
            if let Some(result) = conformance::call_tool(&tool_name) {
                return Ok(result);
            }
        }
        if tool_name != "soma" {
            if let Some(result) = gateway_proxy::call_tool_for_subject_and_scope(
                self.state.application(),
                &tool_name,
                request.arguments.clone(),
                route_scope,
                &execution_context,
            )
            .await
            {
                if result.is_error == Some(true) {
                    trace_summary_event!(
                        warn,
                        trace_resolution,
                        trace_context_conflict,
                        "MCP gateway tool execution failed",
                        tool = %tool_name,
                        action = action_opt.as_deref().unwrap_or_default(),
                    );
                } else {
                    trace_summary_event!(
                        info,
                        trace_resolution,
                        trace_context_conflict,
                        "MCP gateway tool execution completed",
                        tool = %tool_name,
                        action = action_opt.as_deref().unwrap_or_default(),
                    );
                }
                return Ok(result);
            }
            trace_summary_event!(
                warn,
                trace_resolution,
                trace_context_conflict,
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
                trace_resolution,
                trace_context_conflict,
                "MCP tool returned cached response page",
                tool = %tool_name,
                action = empty_action_as_none(&action).unwrap_or_default(),
            );
            return tool_result_from_cached_page(
                self.state.response_pages(),
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
        let started = Instant::now();
        trace_summary_event!(
            info,
            trace_resolution,
            trace_context_conflict,
            "MCP tool execution started",
            tool = %tool_name,
            action = %action,
        );

        match execute_tool(&self.state, &tool_name, arguments, &peer, execution_context).await {
            Ok(result) => {
                trace_summary_event!(
                    info,
                    trace_resolution,
                    trace_context_conflict,
                    "MCP tool execution completed",
                    tool = %tool_name,
                    action = %action,
                    elapsed_ms = started.elapsed().as_millis(),
                );
                tool_result_from_json(
                    result,
                    self.state.response_pages(),
                    response_page,
                    response_paging_options(),
                    &tool_name,
                    empty_action_as_none(&action),
                    continuation_args.as_ref(),
                )
            }
            Err(error) => {
                let application_error = error.downcast_ref::<ApplicationError>();
                if application_error.is_some_and(ApplicationError::is_validation) {
                    trace_summary_event!(
                        warn,
                        trace_resolution,
                        trace_context_conflict,
                        "MCP tool rejected invalid params",
                        tool = %tool_name,
                        action = %action,
                        elapsed_ms = started.elapsed().as_millis(),
                    );
                } else {
                    trace_summary_event!(
                        error,
                        trace_resolution,
                        trace_context_conflict,
                        "MCP tool execution failed",
                        tool = %tool_name,
                        action = %action,
                        elapsed_ms = started.elapsed().as_millis(),
                        service_error_kind = application_error
                            .and_then(ApplicationError::service_error_kind),
                        private_diagnostics = application_error
                            .and_then(ApplicationError::private_diagnostics),
                        error = %error,
                    );
                }
                tool_error_result(application_error_payload(
                    &error,
                    &tool_name,
                    empty_action_as_none(&action),
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
        let auth = require_auth_context(&self.state, &context)?;
        let route_scope = protected_route_scope(&context);
        let execution_context = execution_context(&self.state, &context, auth);
        refresh_file_providers(&self.state)?;
        let soma_allowed = protected_scope_allows_service(route_scope, "soma");
        let mut resources = if soma_allowed {
            let mut resources = vec![schema_resource()];
            resources.extend(
                self.state
                    .application()
                    .list_resources()
                    .iter()
                    .map(rmcp_resource_from_catalog_resource),
            );
            resources
        } else {
            Vec::new()
        };
        resources.extend(
            gateway_proxy::list_resources_for_subject_and_scope(
                self.state.application(),
                route_scope,
                &execution_context,
            )
            .await?,
        );
        if soma_allowed && self.state.config().conformance_fixtures {
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
        let mut resource_templates: Vec<ResourceTemplate> = self
            .state
            .application()
            .list_resource_templates()
            .into_iter()
            .map(|template| {
                let mut built = ResourceTemplate::new(template.uri_template, template.name)
                    .with_description(template.description.clone());
                if let Some(mime_type) = template.mime_type {
                    built = built.with_mime_type(mime_type);
                }
                built
            })
            .collect();
        if self.state.config().conformance_fixtures {
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
        let execution_context = execution_context(&self.state, &context, auth);
        let soma_allowed = protected_scope_allows_service(route_scope, "soma");
        if soma_allowed && self.state.config().conformance_fixtures {
            if let Some(result) = conformance::read_resource(&request.uri) {
                return Ok(result);
            }
        }
        if let Some(result) = gateway_proxy::read_resource_for_subject_and_scope(
            self.state.application(),
            &request.uri,
            route_scope,
            &execution_context,
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

        let output = self
            .state
            .application()
            .read_resource(
                ReadResourceRequest {
                    uri: request.uri.clone(),
                },
                execution_context,
            )
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
        let execution_context = execution_context(&self.state, &context, auth);
        let soma_allowed = protected_scope_allows_service(route_scope, "soma");
        let mut result = if soma_allowed {
            prompts::list_prompts()
        } else {
            ListPromptsResult::default()
        };
        refresh_file_providers(&self.state)?;
        if soma_allowed {
            result.prompts.extend(prompts::provider_prompts(
                &self.state.application().list_prompts(),
            ));
        }
        result.prompts.extend(
            gateway_proxy::list_prompts_for_subject_and_scope(
                self.state.application(),
                route_scope,
                &execution_context,
            )
            .await?,
        );
        if soma_allowed && self.state.config().conformance_fixtures {
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
        let execution_context = execution_context(&self.state, &context, auth);
        let soma_allowed = protected_scope_allows_service(route_scope, "soma");
        if soma_allowed && self.state.config().conformance_fixtures {
            if let Some(result) = conformance::get_prompt(request.clone()) {
                return Ok(result);
            }
        }
        if let Some(result) = gateway_proxy::get_prompt_for_subject_and_scope(
            self.state.application(),
            &request.name,
            request.arguments.clone(),
            route_scope,
            &execution_context,
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
        match self
            .state
            .application()
            .get_prompt(&request.name, &execution_context)
        {
            Ok(prompt) => return Ok(prompts::provider_prompt_result(prompt)),
            Err(error) if error.code == "insufficient_scope" => {
                return Err(ErrorData::invalid_request(
                    format!("forbidden: {}", error.message),
                    None,
                ));
            }
            Err(error) if error.code == "prompt_not_found" => {}
            Err(error) => {
                return Err(ErrorData::internal_error(error.message, None));
            }
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
            self.state.config().server_name.clone(),
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
Homepage: https://soma.dinglebear.ai. Repository: https://github.com/dinglebear-ai/soma. \
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

fn resource_contents_from_output(uri: &str, output: ResourceContent) -> ResourceContents {
    match output {
        ResourceContent::Text { text, mime_type } => {
            let mut contents = ResourceContents::text(text, uri);
            if let Some(mime_type) = mime_type {
                contents = contents.with_mime_type(mime_type);
            }
            contents
        }
        ResourceContent::Blob {
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

/// Maps an application resource failure to the protocol-level
/// `ErrorData` MCP `resources/read` expects — there is no structured
/// tool-result-style "isError" channel for resource reads the way
/// `call_tool` has, so every failure kind maps to `ErrorData`.
fn resource_read_error(uri: &str, error: &ApplicationError) -> ErrorData {
    match error.code.as_str() {
        "unknown_resource" => ErrorData::invalid_params(format!("unknown resource: {uri}"), None),
        "insufficient_scope" => {
            ErrorData::invalid_request(format!("forbidden: {}", error.message), None)
        }
        _ => ErrorData::internal_error(error.message.to_string(), None),
    }
}

// ── tool definition conversion ────────────────────────────────────────────────

fn rmcp_tool_definitions(state: &McpState) -> Result<Vec<Tool>, ErrorData> {
    tool_definitions_for_state(state)
        .into_iter()
        .map(rmcp_tool_from_json)
        .collect()
}

fn refresh_file_providers(state: &McpState) -> Result<(), ErrorData> {
    state
        .application()
        .refresh_providers_in_place()
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))
}

fn tool_definitions_for_state(state: &McpState) -> Vec<Value> {
    let snapshot = state.application().catalog_snapshot();
    tool_definitions(&snapshot.catalogs)
}

fn rmcp_tool_from_json(value: Value) -> Result<Tool, ErrorData> {
    soma_mcp_server::protocol::tool_from_json_definition(value)
}

fn empty_action_as_none(action: &str) -> Option<&str> {
    if action.is_empty() {
        None
    } else {
        Some(action)
    }
}

fn execution_context(
    state: &McpState,
    request: &RequestContext<RoleServer>,
    auth: Option<&AuthContext>,
) -> ExecutionContext {
    state.execution_context(
        Some(principal(auth)),
        trace_context_from_meta(&request.meta),
    )
}

fn execution_context_with_trace(
    state: &McpState,
    auth: Option<&AuthContext>,
    trace: Option<TraceContext>,
) -> ExecutionContext {
    state.execution_context(Some(principal(auth)), trace)
}

/// Resolve trace metadata for one authenticated `call_tool` invocation. `Off`
/// mode returns without ever touching `RequestContext.extensions`.
fn trace_resolution_for_call(
    state: &McpState,
    context: &RequestContext<RoleServer>,
) -> trace_resolution::TraceResolution {
    let mode = state.config().trace_headers;
    if mode == soma_config::TraceHeaderMode::Off {
        return trace_resolution::TraceResolution::from_meta_only(&context.meta);
    }
    let headers = context
        .extensions
        .get::<http::request::Parts>()
        .map(|parts| &parts.headers);
    trace_resolution::resolve_trace_resolution(mode, &context.meta, headers)
}

fn trace_context_from_meta(meta: &rmcp::model::Meta) -> Option<TraceContext> {
    let fields = soma_mcp_server::trace::raw_trace_fields_from_meta(meta, TraceTrust::Untrusted)?;
    Some(TraceContext {
        traceparent: fields.traceparent,
        tracestate: fields.tracestate,
    })
}

#[cfg(test)]
#[path = "rmcp_server_tests.rs"]
mod tests;
