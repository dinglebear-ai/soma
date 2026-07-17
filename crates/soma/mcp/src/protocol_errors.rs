use rmcp::{
    model::{CallToolResult, ContentBlock},
    ErrorData,
};
use serde_json::{json, Value};

use soma_application::{ApplicationError, ApplicationErrorDetails};
use soma_domain::token_limit::MAX_RESPONSE_BYTES;

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

pub(super) fn application_error_payload(
    error: &anyhow::Error,
    tool: &str,
    action: Option<&str>,
) -> Value {
    if let Some(error) = error.downcast_ref::<ApplicationError>() {
        return match error.details.as_ref() {
            ApplicationErrorDetails::Provider {
                schema_version,
                provider,
                action: provider_action,
                provider_error_kind,
            } => json!({
                "kind": "mcp_tool_error",
                "schema_version": schema_version,
                "code": error.code,
                "tool": tool,
                "provider": provider,
                "action": provider_action.as_deref().or(action),
                "message": error.message,
                "retryable": error.retryable,
                "remediation": error.remediation,
                "provider_error_kind": provider_error_kind,
            }),
            ApplicationErrorDetails::Service {
                schema_version,
                service_error_kind,
                field,
                bad_value,
                expected_pattern,
                reason_kind,
                available_actions,
            } => {
                let mut payload = json!({
                    "kind": "mcp_tool_error",
                    "schema_version": schema_version,
                    "code": error.code,
                    "tool": tool,
                    "action": action,
                    "message": error.message,
                    "retryable": error.retryable,
                    "remediation": error.remediation,
                    "service_error_kind": service_error_kind,
                });
                add_optional_error_field(&mut payload, "field", field.as_deref());
                add_optional_error_field(&mut payload, "bad_value", bad_value.as_deref());
                add_optional_error_field(
                    &mut payload,
                    "expected_pattern",
                    expected_pattern.as_deref(),
                );
                add_optional_error_field(&mut payload, "reason_kind", reason_kind.as_deref());
                if !available_actions.is_empty() {
                    payload["available_actions"] = json!(available_actions);
                }
                payload
            }
            ApplicationErrorDetails::Generic => json!({
                "kind": "mcp_tool_error",
                "schema_version": 1,
                "code": error.code,
                "tool": tool,
                "action": action,
                "message": error.message,
                "retryable": error.retryable,
                "remediation": error.remediation,
            }),
        };
    }
    soma_domain::errors::ToolError::execution(error).to_mcp_payload(tool, action)
}

fn add_optional_error_field(payload: &mut Value, field: &str, value: Option<&str>) {
    if let Some(value) = value {
        payload[field] = json!(value);
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
#[path = "protocol_errors_tests.rs"]
mod tests;
