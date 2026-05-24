use serde_json::json;

use crate::{
    actions::{required_scope_for_action, ValidationError, READ_SCOPE, WRITE_SCOPE},
    token_limit::MAX_RESPONSE_BYTES,
};

use super::{
    internal_tool_error_message, reject_unknown_action_before_scope, scope_satisfied,
    tool_error_result, tool_result_from_json, unknown_action_payload, unknown_tool_error,
    validation_error_payload,
};

fn scopes(s: &[&str]) -> Vec<String> {
    s.iter().map(|x| x.to_string()).collect()
}

#[test]
fn read_scope_satisfies_read_requirement() {
    assert!(scope_satisfied(&scopes(&[READ_SCOPE]), READ_SCOPE));
}

#[test]
fn write_scope_satisfies_read_requirement() {
    assert!(
        scope_satisfied(&scopes(&[WRITE_SCOPE]), READ_SCOPE),
        "write scope should satisfy read requirement (write ⊇ read)"
    );
}

#[test]
fn empty_scopes_denied() {
    assert!(!scope_satisfied(&[], READ_SCOPE));
}

#[test]
fn unrelated_scope_denied() {
    assert!(!scope_satisfied(&scopes(&["other:scope"]), READ_SCOPE));
}

#[test]
fn read_scope_does_not_satisfy_write() {
    assert!(
        !scope_satisfied(&scopes(&[READ_SCOPE]), WRITE_SCOPE),
        "read scope must not satisfy write requirement"
    );
}

#[test]
fn greet_requires_read_scope() {
    assert_eq!(required_scope_for_action("greet"), Some(READ_SCOPE));
}

#[test]
fn help_requires_no_scope() {
    assert_eq!(required_scope_for_action("help"), None);
}

#[test]
fn unknown_action_gets_deny_scope() {
    use crate::actions::DENY_SCOPE;
    assert_eq!(
        required_scope_for_action("nonexistent_action"),
        Some(DENY_SCOPE)
    );
}

#[test]
fn unknown_action_is_rejected_as_validation_before_scope() {
    let error = reject_unknown_action_before_scope("nonexistent_action")
        .expect_err("unknown action should be invalid params");
    assert!(error.message.contains("unknown example action"));
}

#[test]
fn internal_tool_errors_include_stable_kind() {
    let message = internal_tool_error_message("status");
    assert!(message.contains("kind=execution_error"));
    assert!(message.contains("action='status'"));
}

#[test]
fn tool_result_from_json_applies_response_cap() {
    let result = tool_result_from_json(json!({
        "payload": "x".repeat(MAX_RESPONSE_BYTES + 1)
    }))
    .expect("tool result should serialize");
    let text = result.content[0]
        .raw
        .as_text()
        .expect("tool result should contain text")
        .text
        .as_str();
    assert!(text.contains("[TRUNCATED"));
}

#[test]
fn validation_errors_become_structured_tool_errors() {
    let error = anyhow::Error::from(ValidationError::MissingField {
        field: "message".to_owned(),
    });
    let payload = validation_error_payload("example", Some("echo"), &error);
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
fn unknown_actions_become_retryable_tool_errors() {
    let result = tool_error_result(unknown_action_payload("example", "missing"))
        .expect("unknown action payload should serialize");

    assert_eq!(result.is_error, Some(true));
    let structured = result.structured_content.as_ref().unwrap();
    assert_eq!(structured["code"], "unknown_action");
    assert_eq!(structured["bad_value"], "missing");
    assert!(structured["available_actions"]
        .as_array()
        .unwrap()
        .contains(&json!("help")));
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
    let payload = super::execution_error_payload("example", Some("status"));

    assert_eq!(
        payload,
        json!({
            "kind": "mcp_tool_error",
            "schema_version": 1,
            "code": "execution_error",
            "tool": "example",
            "action": "status",
            "message": "Tool execution failed. Check server logs for details.",
            "retryable": true,
            "remediation": "Check service configuration and upstream availability, then retry. Use action=status or action=help for diagnostics.",
        })
    );
}
