//! Support functions for the `ServerHandler` impl in `rmcp_server.rs`:
//! server metadata/instructions, resource conversion, tool-definition
//! assembly, and per-call execution/trace context. Split out to keep
//! `rmcp_server.rs` under the PATTERNS.md module size hard limit.
use rmcp::{
    ErrorData, RoleServer,
    model::{
        CacheScope, ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult,
        ListToolsResult, ReadResourceResponse, Resource, ResourceContents, ResourceTemplate, Tool,
    },
    service::RequestContext,
};
use rmcp_traces::TraceTrust;
use serde_json::Value;
use soma_application::{ApplicationError, ExecutionContext, ResourceContent};
use soma_domain::{TraceContext, token_limit::MAX_RESPONSE_BYTES};
use soma_mcp_server::response_paging::ResponsePagingOptions;
use soma_provider_core::ProviderResource;

use crate::ACTION_DISCRIMINATOR_FIELD;
use crate::rmcp_auth::{AuthContext, principal};
use crate::schemas::tool_definitions_for_catalogs as tool_definitions;
use crate::state::McpState;
use crate::trace_resolution;

pub(super) fn task_application_error(error: ApplicationError) -> ErrorData {
    if error.code == "task_missing" || error.code == "not_found" {
        ErrorData::invalid_params(error.message, None)
    } else if error.code == "insufficient_scope" {
        ErrorData::invalid_request(format!("forbidden: {}", error.message), None)
    } else {
        ErrorData::internal_error(error.message, None)
    }
}

pub(super) fn response_paging_options() -> ResponsePagingOptions {
    ResponsePagingOptions {
        max_response_bytes: MAX_RESPONSE_BYTES,
        action_discriminator_field: ACTION_DISCRIMINATOR_FIELD,
    }
}

pub(super) const SERVER_INSTRUCTIONS: &str = "\
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
pub(super) const SCHEMA_RESOURCE_URI: &str = "soma://schema/mcp-tool";

pub(super) fn schema_resource() -> Resource {
    Resource::new(SCHEMA_RESOURCE_URI, "soma tool schema")
        .with_description("JSON schema for the Soma MCP tool and its action-based parameters")
        .with_mime_type("application/json")
}

pub(super) fn rmcp_resource_from_catalog_resource(resource: &ProviderResource) -> Resource {
    let mut built = Resource::new(resource.uri_template.clone(), resource.name.clone())
        .with_description(resource.description.clone());
    if let Some(mime_type) = &resource.mime_type {
        built = built.with_mime_type(mime_type.clone());
    }
    built
}

pub(super) fn resource_contents_from_output(
    uri: &str,
    output: ResourceContent,
) -> ResourceContents {
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
pub(super) fn resource_read_error(uri: &str, error: &ApplicationError) -> ErrorData {
    match error.code.as_str() {
        "unknown_resource" => ErrorData::invalid_params(format!("unknown resource: {uri}"), None),
        "insufficient_scope" => {
            ErrorData::invalid_request(format!("forbidden: {}", error.message), None)
        }
        _ => ErrorData::internal_error(error.message.to_string(), None),
    }
}

// ── tool definition conversion ────────────────────────────────────────────────

pub(super) fn rmcp_tool_definitions(state: &McpState) -> Result<Vec<Tool>, ErrorData> {
    tool_definitions_for_state(state)
        .into_iter()
        .map(rmcp_tool_from_json)
        .collect()
}

pub(super) async fn refresh_file_providers(state: &McpState) -> Result<(), ErrorData> {
    state
        .application()
        .refresh_providers_in_place_async()
        .await
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))
}

pub(super) fn tool_definitions_for_state(state: &McpState) -> Vec<Value> {
    let snapshot = state.application().catalog_snapshot();
    tool_definitions(&snapshot.catalogs)
}

pub(super) fn rmcp_tool_from_json(value: Value) -> Result<Tool, ErrorData> {
    soma_mcp_server::protocol::tool_from_json_definition(value)
}

pub(super) fn empty_action_as_none(action: &str) -> Option<&str> {
    if action.is_empty() {
        None
    } else {
        Some(action)
    }
}

pub(super) fn execution_context(
    state: &McpState,
    request: &RequestContext<RoleServer>,
    auth: Option<&AuthContext>,
) -> ExecutionContext {
    state.execution_context(
        Some(principal(auth)),
        trace_context_from_meta(&request.meta),
    )
}

pub(super) fn request_meta_value(context: &RequestContext<RoleServer>) -> Option<Value> {
    if context.meta.is_empty() {
        None
    } else {
        serde_json::to_value(&context.meta).ok()
    }
}

pub(super) fn execution_context_with_trace(
    state: &McpState,
    auth: Option<&AuthContext>,
    trace: Option<TraceContext>,
) -> ExecutionContext {
    state.execution_context(Some(principal(auth)), trace)
}

/// Resolve trace metadata for one authenticated `call_tool` invocation. `Off`
/// mode returns without ever touching `RequestContext.extensions`.
pub(super) fn trace_resolution_for_call(
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

pub(super) fn trace_context_from_meta(meta: &rmcp::model::Meta) -> Option<TraceContext> {
    let fields = soma_mcp_server::trace::raw_trace_fields_from_meta(meta, TraceTrust::Untrusted)?;
    Some(TraceContext {
        traceparent: fields.traceparent,
        tracestate: fields.tracestate,
    })
}

// ── cache-scope wrappers ──────────────────────────────────────────────────────

pub(super) fn private_tools_result(tools: Vec<Tool>) -> ListToolsResult {
    ListToolsResult::with_all_items(tools)
        .with_ttl_ms(0)
        .with_cache_scope(CacheScope::Private)
}

pub(super) fn private_resources_result(resources: Vec<Resource>) -> ListResourcesResult {
    ListResourcesResult::with_all_items(resources)
        .with_ttl_ms(0)
        .with_cache_scope(CacheScope::Private)
}

pub(super) fn private_resource_templates_result(
    resource_templates: Vec<ResourceTemplate>,
) -> ListResourceTemplatesResult {
    ListResourceTemplatesResult::with_all_items(resource_templates)
        .with_ttl_ms(0)
        .with_cache_scope(CacheScope::Private)
}

pub(super) fn private_prompts_result(mut result: ListPromptsResult) -> ListPromptsResult {
    result.ttl_ms = Some(0);
    result.cache_scope = Some(CacheScope::Private);
    result
}

pub(super) fn private_dynamic_read_response(
    response: ReadResourceResponse,
) -> ReadResourceResponse {
    match response {
        ReadResourceResponse::Complete(result) => ReadResourceResponse::Complete(
            result.with_ttl_ms(0).with_cache_scope(CacheScope::Private),
        ),
        other => other,
    }
}

#[cfg(test)]
#[path = "support_tests.rs"]
mod tests;
