use super::*;

#[test]
fn small_payload_round_trips_unchanged() {
    let value = json!({"kind": "mcp_tool_error", "code": "boom", "message": "bad"});
    let result = tool_error_result(value.clone(), 4096).expect("small payload should serialize");

    assert_eq!(result.is_error, Some(true));
    assert_eq!(result.structured_content.as_ref(), Some(&value));
}

#[test]
fn oversized_payload_returns_overflow_envelope() {
    let value = json!({
        "kind": "mcp_tool_error",
        "code": "huge_error",
        "message": "x".repeat(200),
    });
    let result = tool_error_result(value, 32).expect("oversized payload should still serialize");

    let structured = result
        .structured_content
        .as_ref()
        .expect("structured content should be present");
    assert_eq!(structured["kind"], "mcp_tool_error");
    assert_eq!(structured["code"], "error_payload_too_large");
    assert_eq!(structured["original_code"], "huge_error");
    assert!(structured["serialized_bytes"].as_u64().unwrap() > 32);
    assert_eq!(structured["max_response_bytes"], 32);

    let text = result.content[0]
        .as_text()
        .expect("overflow result should contain text")
        .text
        .as_str();
    let parsed: Value = serde_json::from_str(text).expect("overflow text should remain valid JSON");
    assert_eq!(&parsed, structured);
}

#[test]
fn unknown_tool_error_lists_available_tools() {
    let error = unknown_tool_error("bad_tool", &["soma"]);

    assert!(error.message.contains("unknown tool: bad_tool"));
    assert!(error.message.contains("soma"));
    let data = error.data.expect("unknown tool error should include data");
    assert_eq!(data["kind"], "mcp_protocol_error");
    assert_eq!(data["code"], "unknown_tool");
    assert_eq!(data["tool"], "bad_tool");
    assert_eq!(data["available_tools"], json!(["soma"]));
}

#[test]
fn unknown_tool_error_omits_available_tools_suffix_when_empty() {
    let error = unknown_tool_error("bad_tool", &[]);

    assert_eq!(error.message, "unknown tool: bad_tool");
}
