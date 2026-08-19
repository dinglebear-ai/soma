use serde_json::json;

use soma_application::{ApplicationError, PortError};
use soma_domain::token_limit::MAX_RESPONSE_BYTES;

use crate::assert_result_has_no_meta;

use super::{application_error_payload, tool_error_result, unknown_tool_error};

#[test]
fn oversized_tool_errors_return_a_valid_bounded_envelope() {
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
        serde_json::from_str(text).expect("overflow text should remain valid JSON");

    assert_result_has_no_meta(&result);
    assert_eq!(result.is_error, Some(true));
    assert_eq!(parsed["code"], "error_payload_too_large");
    assert_eq!(parsed["original_code"], "huge_error");
    assert!(parsed["serialized_bytes"].as_u64().unwrap() > MAX_RESPONSE_BYTES as u64);
    assert_eq!(result.structured_content.as_ref(), Some(&parsed));
}

#[test]
fn port_error_details_are_preserved_in_public_mcp_payloads() {
    let mut port = PortError::new("tool_execution_failed", "bad input");
    port.retryable = false;
    port.remediation = "revise the request".to_owned();
    port.details = Some(json!({
        "contract_version": 1,
        "kind": "invalid_param",
        "recovery": {
            "action": "revise_and_retry",
            "same_arguments": "discouraged"
        },
        "side_effects": "possible"
    }));
    let error = anyhow::Error::new(ApplicationError::from(port));

    let payload = application_error_payload(&error, "soma", Some("gateway.call"));
    assert_eq!(payload["code"], "tool_execution_failed");
    assert_eq!(payload["retryable"], false);
    assert_eq!(payload["remediation"], "revise the request");
    assert_eq!(payload["details"]["kind"], "invalid_param");
    assert_eq!(
        payload["details"]["recovery"]["same_arguments"],
        "discouraged"
    );
    assert_eq!(payload["details"]["side_effects"], "possible");
}

#[test]
fn unknown_tool_errors_include_machine_readable_protocol_data() {
    let error = unknown_tool_error("bad_tool");
    let data = error.data.expect("unknown tool should include data");

    assert_eq!(data["kind"], "mcp_protocol_error");
    assert_eq!(data["code"], "unknown_tool");
    assert_eq!(data["tool"], "bad_tool");
    assert_eq!(data["available_tools"], json!(["soma"]));
}
