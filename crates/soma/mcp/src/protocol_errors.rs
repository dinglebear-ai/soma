use rmcp::ErrorData;
use serde_json::{json, Value};

use soma_application::{ApplicationError, ApplicationErrorDetails};
use soma_domain::token_limit::MAX_RESPONSE_BYTES;
use soma_mcp_server::error_result;

pub(super) fn tool_error_result(value: Value) -> Result<rmcp::model::CallToolResult, ErrorData> {
    error_result::tool_error_result(value, MAX_RESPONSE_BYTES)
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
    error_result::unknown_tool_error(tool_name, &["soma"])
}

#[cfg(test)]
#[path = "protocol_errors_tests.rs"]
mod tests;
