use std::{borrow::Cow, sync::Arc};

use rmcp::{
    model::{CallToolResult, ContentBlock, Resource, ResourceContents, Tool},
    ErrorData,
};
use serde_json::{json, Value};
use soma_contracts::{providers::ProviderResource, token_limit::MAX_RESPONSE_BYTES};
use soma_runtime::server::AppState;
use soma_service::{ProviderError, ResourceReadOutput};

use crate::schemas::tool_definitions_for_catalogs as tool_definitions;

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
    output: ResourceReadOutput,
) -> ResourceContents {
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

/// Maps a provider resource-read failure to protocol-level MCP `ErrorData`.
pub(super) fn resource_read_error(uri: &str, error: &ProviderError) -> ErrorData {
    match error.code.as_ref() {
        "unknown_resource" => ErrorData::invalid_params(format!("unknown resource: {uri}"), None),
        "insufficient_scope" => {
            ErrorData::invalid_request(format!("forbidden: {}", error.message), None)
        }
        _ => ErrorData::internal_error(error.message.to_string(), None),
    }
}

pub(super) fn rmcp_tool_definitions(state: &AppState) -> Result<Vec<Tool>, ErrorData> {
    tool_definitions_for_state(state)
        .into_iter()
        .map(rmcp_tool_from_json)
        .collect()
}

pub(super) fn refresh_file_providers(state: &AppState) -> Result<(), ErrorData> {
    state
        .provider_registry
        .refresh_file_providers()
        .map(|_| ())
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))
}

pub(super) fn tool_definitions_for_state(state: &AppState) -> Vec<Value> {
    let snapshot = state.provider_registry.snapshot();
    tool_definitions(&snapshot.catalogs)
}

pub(super) fn rmcp_tool_from_json(value: Value) -> Result<Tool, ErrorData> {
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

pub(super) fn tool_error_result(value: Value) -> Result<CallToolResult, ErrorData> {
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

pub(super) fn provider_error_payload(
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

pub(super) fn empty_action_as_none(action: &str) -> Option<&str> {
    if action.is_empty() {
        None
    } else {
        Some(action)
    }
}

pub(super) fn unknown_tool_error(tool_name: &str) -> ErrorData {
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
#[path = "rmcp_adapters_tests.rs"]
mod tests;
