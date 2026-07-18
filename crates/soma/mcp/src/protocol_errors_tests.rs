use serde_json::json;

use soma_domain::token_limit::MAX_RESPONSE_BYTES;

use crate::assert_result_has_no_meta;

use super::{tool_error_result, unknown_tool_error};

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
fn unknown_tool_errors_include_machine_readable_protocol_data() {
    let error = unknown_tool_error("bad_tool");
    let data = error.data.expect("unknown tool should include data");

    assert_eq!(data["kind"], "mcp_protocol_error");
    assert_eq!(data["code"], "unknown_tool");
    assert_eq!(data["tool"], "bad_tool");
    assert_eq!(data["available_tools"], json!(["soma"]));
}
