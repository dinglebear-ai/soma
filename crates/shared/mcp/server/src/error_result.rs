//! Reusable protocol-level tool-error result shaping.
//!
//! Bridges a structured JSON error payload into an MCP `CallToolResult` with
//! `isError: true`, and builds the generic "unknown tool" protocol error
//! every action-dispatch or router-style MCP server eventually needs.

use rmcp::{
    model::{CallToolResult, ContentBlock},
    ErrorData,
};
use serde_json::{json, Value};

/// Wrap a structured tool-error JSON payload into an MCP `CallToolResult`,
/// truncating down to a small overflow envelope when the serialized payload
/// would exceed `max_response_bytes` so callers never emit invalid,
/// mid-value-truncated JSON.
pub fn tool_error_result(
    value: Value,
    max_response_bytes: usize,
) -> Result<CallToolResult, ErrorData> {
    let text = serde_json::to_string(&value)
        .map_err(|e| ErrorData::internal_error(format!("serialization error: {e}"), None))?;
    let (payload, text) = if text.len() <= max_response_bytes {
        (value, text)
    } else {
        let payload = error_overflow_payload(&value, text.len(), max_response_bytes);
        let text = serde_json::to_string(&payload)
            .map_err(|e| ErrorData::internal_error(format!("serialization error: {e}"), None))?;
        (payload, text)
    };
    let mut result = CallToolResult::structured_error(payload);
    result.content = vec![ContentBlock::text(text)];
    Ok(result)
}

fn error_overflow_payload(
    value: &Value,
    serialized_bytes: usize,
    max_response_bytes: usize,
) -> Value {
    json!({
        "kind": "mcp_tool_error",
        "schema_version": 1,
        "code": "error_payload_too_large",
        "original_kind": value.get("kind").cloned().unwrap_or(Value::Null),
        "original_code": value.get("code").cloned().unwrap_or(Value::Null),
        "message": "Tool error payload exceeded the MCP response size limit. The original JSON was not returned to avoid invalid truncated JSON.",
        "retryable": true,
        "serialized_bytes": serialized_bytes,
        "max_response_bytes": max_response_bytes,
        "remediation": "Retry with narrower arguments. If this repeats, inspect server logs for the original error details.",
    })
}

/// Build a generic "unknown tool" protocol error for `call_tool`/`list_tools`
/// dispatch, naming the offending tool and (when non-empty) the tools the
/// server does expose.
pub fn unknown_tool_error(tool_name: &str, available_tools: &[&str]) -> ErrorData {
    let message = if available_tools.is_empty() {
        format!("unknown tool: {tool_name}")
    } else {
        format!(
            "unknown tool: {tool_name}; available tools: {}",
            available_tools.join(", ")
        )
    };
    ErrorData::invalid_params(
        message,
        Some(json!({
            "kind": "mcp_protocol_error",
            "schema_version": 1,
            "code": "unknown_tool",
            "tool": tool_name,
            "available_tools": available_tools,
            "retryable": true,
            "remediation": "Call tools/list, then retry with one of the advertised tool names.",
        })),
    )
}

#[cfg(test)]
#[path = "error_result_tests.rs"]
mod tests;
