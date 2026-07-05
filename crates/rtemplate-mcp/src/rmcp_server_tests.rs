use serde_json::json;

use rtemplate_contracts::{
    actions::{ExampleAction, ValidationError},
    token_limit::MAX_RESPONSE_BYTES,
};
use rtemplate_service::classify_service_error;

use super::{tool_error_result, unknown_tool_error};

#[test]
fn validation_errors_become_structured_tool_errors() {
    let error = anyhow::Error::from(ValidationError::MissingField {
        field: "message".to_owned(),
    });
    let payload = classify_service_error(&error).to_mcp_payload("example", Some("echo"));
    let result = tool_error_result(payload).expect("tool error should serialize");

    assert_eq!(result.is_error, Some(true));
    let structured = result
        .structured_content
        .as_ref()
        .expect("structured content should be present");
    assert_eq!(structured["kind"], "mcp_tool_error");
    assert_eq!(structured["schema_version"], 1);
    assert_eq!(structured["code"], "missing_field");
    assert_eq!(structured["tool"], "example");
    assert_eq!(structured["action"], "echo");
    assert_eq!(structured["field"], "message");
    assert!(structured["remediation"]
        .as_str()
        .unwrap_or_default()
        .contains("action=help"));
}

#[test]
fn parser_validation_errors_become_structured_tool_errors() {
    let error = ExampleAction::from_mcp_args(&json!({
        "action": "echo"
    }))
    .expect_err("missing echo message should fail parsing");
    let payload = classify_service_error(&error).to_mcp_payload("example", Some("echo"));
    let result = tool_error_result(payload).expect("tool error should serialize");

    assert_eq!(result.is_error, Some(true));
    let structured = result
        .structured_content
        .as_ref()
        .expect("structured content should be present");
    assert_eq!(structured["kind"], "mcp_tool_error");
    assert_eq!(structured["code"], "missing_field");
    assert_eq!(structured["tool"], "example");
    assert_eq!(structured["action"], "echo");
    assert_eq!(structured["field"], "message");
}

#[test]
fn oversized_tool_errors_return_valid_overflow_envelope() {
    let result = tool_error_result(json!({
        "kind": "mcp_tool_error",
        "schema_version": 1,
        "code": "huge_error",
        "message": "x".repeat(MAX_RESPONSE_BYTES + 1),
    }))
    .expect("tool error should serialize");
    let text = result.content[0]
        .as_text()
        .expect("tool error should contain text")
        .text
        .as_str();
    let parsed: serde_json::Value =
        serde_json::from_str(text).expect("overflow error text should remain valid JSON");

    assert_eq!(result.is_error, Some(true));
    assert_eq!(parsed["kind"], "mcp_tool_error");
    assert_eq!(parsed["code"], "error_payload_too_large");
    assert_eq!(parsed["original_code"], "huge_error");
    assert!(parsed["serialized_bytes"].as_u64().unwrap() > MAX_RESPONSE_BYTES as u64);
    assert_eq!(result.structured_content.as_ref(), Some(&parsed));
}

#[test]
fn unknown_tool_stays_protocol_error_with_structured_data() {
    let error = unknown_tool_error("bad_tool");

    assert!(error.message.contains("unknown tool"));
    let data = error
        .data
        .expect("unknown tool should include structured data");
    assert_eq!(data["kind"], "mcp_protocol_error");
    assert_eq!(data["code"], "unknown_tool");
    assert_eq!(data["tool"], "bad_tool");
    assert_eq!(data["available_tools"], json!(["example"]));
}

#[test]
fn execution_errors_do_not_expose_raw_error_text() {
    let raw_error = anyhow::anyhow!("upstream timeout talking to secret-api-key");
    let payload = classify_service_error(&raw_error).to_mcp_payload("example", Some("status"));

    assert_eq!(
        payload,
        json!({
            "kind": "mcp_tool_error",
            "schema_version": 1,
            "code": "execution_error",
            "service_error_kind": "timeout",
            "reason_kind": "timeout",
            "tool": "example",
            "action": "status",
            "message": "Tool execution failed. Check server logs for details.",
            "retryable": true,
            "remediation": "Check service configuration and upstream availability, then retry. Use action=status or action=help for diagnostics.",
        })
    );
    assert!(!payload.to_string().contains("secret-api-key"));
}
