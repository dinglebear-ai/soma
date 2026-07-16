use serde_json::json;

use crate::assert_result_has_no_meta;

use super::{
    response_page_request, tool_result_from_json, ResponsePageRequest, ResponsePageStore,
    ResponsePagingOptions, DEFAULT_MAX_RESPONSE_BYTES,
};

fn result_text(result: &rmcp::model::CallToolResult) -> &str {
    result.content[0]
        .as_text()
        .expect("tool result should contain text")
        .text
        .as_str()
}

#[test]
fn tool_result_from_json_adds_action_discriminator() {
    let store = ResponsePageStore::default();
    let result = tool_result_from_json(
        json!({ "status": "ok" }),
        &store,
        ResponsePageRequest::default(),
        ResponsePagingOptions::default(),
        "soma",
        Some("status"),
        None,
    )
    .expect("tool result should serialize");
    let text = result_text(&result);
    let parsed: serde_json::Value =
        serde_json::from_str(text).expect("tool text should remain valid JSON");

    assert_result_has_no_meta(&result);
    assert_eq!(parsed["status"], "ok");
    assert_eq!(parsed["_action"], "status");
    assert_eq!(result.structured_content.as_ref(), Some(&parsed));
}

#[test]
fn tool_result_from_json_returns_scrollable_page_envelope() {
    let store = ResponsePageStore::default();
    let result = tool_result_from_json(
        json!({
            "payload": "x".repeat(DEFAULT_MAX_RESPONSE_BYTES + 1)
        }),
        &store,
        ResponsePageRequest::default(),
        ResponsePagingOptions::default(),
        "soma",
        Some("status"),
        None,
    )
    .expect("tool result should serialize");
    let text = result_text(&result);
    let parsed: serde_json::Value =
        serde_json::from_str(text).expect("paged text should remain valid JSON");

    assert_result_has_no_meta(&result);
    assert_eq!(parsed["kind"], "mcp_response_page");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["code"], "response_page");
    assert_eq!(parsed["truncated"], false);
    assert_eq!(parsed["page"]["offset"], 0);
    assert_eq!(parsed["page"]["has_more"], true);
    assert_eq!(parsed["continuation"]["arguments"]["action"], "status");
    assert!(parsed["continuation"]["arguments"]["_response_cursor"]
        .as_str()
        .unwrap()
        .starts_with("rsp_"));
    assert!(
        parsed["continuation"]["arguments"]["_response_offset"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(parsed["serialized_bytes"].as_u64().unwrap() > DEFAULT_MAX_RESPONSE_BYTES as u64);
    assert!(!text.contains("[TRUNCATED"));
    assert_eq!(result.structured_content.as_ref(), Some(&parsed));
}

#[test]
fn tool_result_from_json_returns_requested_continuation_page() {
    let store = ResponsePageStore::default();
    let first = tool_result_from_json(
        json!({
            "payload": "x".repeat(DEFAULT_MAX_RESPONSE_BYTES + 1)
        }),
        &store,
        ResponsePageRequest::default(),
        ResponsePagingOptions::default(),
        "soma",
        Some("status"),
        None,
    )
    .expect("first page should serialize");
    let first_payload: serde_json::Value = serde_json::from_str(result_text(&first)).unwrap();
    let next_offset = first_payload["continuation"]["arguments"]["_response_offset"]
        .as_u64()
        .expect("first page should expose next offset") as usize;
    let cursor = first_payload["continuation"]["arguments"]["_response_cursor"]
        .as_str()
        .expect("first page should expose response cursor")
        .to_owned();

    let second = tool_result_from_json(
        json!({ "this": "would be a different re-executed response" }),
        &store,
        ResponsePageRequest {
            cursor: Some(cursor),
            offset: next_offset,
            page_bytes: 1024,
        },
        ResponsePagingOptions::default(),
        "soma",
        Some("status"),
        None,
    )
    .expect("second page should serialize");
    let second_payload: serde_json::Value = serde_json::from_str(result_text(&second)).unwrap();

    assert_result_has_no_meta(&first);
    assert_result_has_no_meta(&second);
    assert_eq!(second_payload["kind"], "mcp_response_page");
    assert_eq!(second_payload["page"]["offset"], next_offset);
    assert_eq!(second_payload["page"]["page_bytes"], 1024);
    assert_ne!(second_payload["content"], first_payload["content"]);
    assert!(!second_payload["content"]
        .as_str()
        .expect("page content should be text")
        .contains("would be a different re-executed response"));
}

#[test]
fn response_page_cursor_rejects_missing_or_expired_cursor() {
    let store = ResponsePageStore::default();
    let error = tool_result_from_json(
        json!({ "payload": "this value should not be executed" }),
        &store,
        ResponsePageRequest {
            cursor: Some("rsp_missing".to_owned()),
            offset: 1,
            page_bytes: 1024,
        },
        ResponsePagingOptions::default(),
        "soma",
        Some("status"),
        None,
    )
    .expect_err("missing cursor should be rejected");

    let data = error.data.expect("error should include structured data");
    assert_eq!(data["kind"], "mcp_protocol_error");
    assert_eq!(data["code"], "response_cursor_not_found");
    assert_eq!(data["field"], "_response_cursor");
}

#[test]
fn response_page_cursor_handles_out_of_range_offsets() {
    let store = ResponsePageStore::default();
    let first = tool_result_from_json(
        json!({
            "payload": "x".repeat(DEFAULT_MAX_RESPONSE_BYTES + 1)
        }),
        &store,
        ResponsePageRequest::default(),
        ResponsePagingOptions::default(),
        "soma",
        Some("status"),
        None,
    )
    .expect("first page should serialize");
    let first_payload: serde_json::Value = serde_json::from_str(result_text(&first)).unwrap();
    let cursor = first_payload["continuation"]["arguments"]["_response_cursor"]
        .as_str()
        .expect("first page should expose response cursor")
        .to_owned();
    let serialized_bytes = first_payload["serialized_bytes"]
        .as_u64()
        .expect("first page should expose size") as usize;

    let result = tool_result_from_json(
        json!({ "payload": "this value should not be executed" }),
        &store,
        ResponsePageRequest {
            cursor: Some(cursor),
            offset: serialized_bytes + 100,
            page_bytes: 1024,
        },
        ResponsePagingOptions::default(),
        "soma",
        Some("status"),
        None,
    )
    .expect("out of range continuation should serialize");
    let payload: serde_json::Value = serde_json::from_str(result_text(&result)).unwrap();

    assert_result_has_no_meta(&first);
    assert_result_has_no_meta(&result);
    assert_eq!(payload["kind"], "mcp_response_page");
    assert_eq!(payload["page"]["offset"], serialized_bytes);
    assert_eq!(payload["page"]["has_more"], false);
    assert!(payload["content"].as_str().unwrap().is_empty());
    assert!(payload["continuation"].is_null());
}

#[test]
fn response_page_continuation_preserves_original_arguments() {
    let store = ResponsePageStore::default();
    let mut args = serde_json::Map::new();
    args.insert("action".to_owned(), json!("echo"));
    args.insert("message".to_owned(), json!("hello from original args"));

    let result = tool_result_from_json(
        json!({
            "payload": "x".repeat(DEFAULT_MAX_RESPONSE_BYTES + 1)
        }),
        &store,
        ResponsePageRequest::default(),
        ResponsePagingOptions::default(),
        "soma",
        Some("echo"),
        Some(&args),
    )
    .expect("tool result should serialize");
    let parsed: serde_json::Value = serde_json::from_str(result_text(&result)).unwrap();

    assert_result_has_no_meta(&result);
    assert_eq!(parsed["continuation"]["arguments"]["action"], "echo");
    assert_eq!(
        parsed["continuation"]["arguments"]["message"],
        "hello from original args"
    );
    assert!(parsed["continuation"]["arguments"]["_response_cursor"].is_string());
    assert!(
        parsed["continuation"]["arguments"]["_response_offset"]
            .as_u64()
            .unwrap()
            > 0
    );
}

#[test]
fn response_page_request_rejects_offset_without_cursor() {
    let args = serde_json::Map::from_iter([
        ("action".to_owned(), json!("status")),
        ("_response_offset".to_owned(), json!(1)),
    ]);

    let error = response_page_request(Some(&args)).expect_err("offset needs cursor");
    let data = error.data.expect("error should include structured data");
    assert_eq!(data["kind"], "mcp_protocol_error");
    assert_eq!(data["code"], "missing_response_cursor");
    assert_eq!(data["field"], "_response_cursor");
}

#[test]
fn response_page_request_rejects_oversized_cursor_before_cloning() {
    let args = serde_json::Map::from_iter([
        ("action".to_owned(), json!("status")),
        ("_response_cursor".to_owned(), json!("x".repeat(257))),
        ("_response_offset".to_owned(), json!(1)),
    ]);

    let error = response_page_request(Some(&args)).expect_err("cursor cap should be enforced");
    let data = error.data.expect("error should include structured data");
    assert_eq!(data["kind"], "mcp_protocol_error");
    assert_eq!(data["code"], "response_cursor_too_long");
    assert_eq!(data["field"], "_response_cursor");
}

#[test]
fn response_page_request_rejects_bad_types_and_zero_page_size() {
    let bad_cursor_args = serde_json::Map::from_iter([
        ("action".to_owned(), json!("status")),
        ("_response_cursor".to_owned(), json!(42)),
    ]);
    let bad_cursor = response_page_request(Some(&bad_cursor_args)).unwrap_err();
    assert_eq!(bad_cursor.data.unwrap()["code"], "invalid_response_cursor");

    let bad_offset_args = serde_json::Map::from_iter([
        ("action".to_owned(), json!("status")),
        ("_response_cursor".to_owned(), json!("rsp_test")),
        ("_response_offset".to_owned(), json!("one")),
    ]);
    let bad_offset = response_page_request(Some(&bad_offset_args)).unwrap_err();
    assert_eq!(
        bad_offset.data.unwrap()["code"],
        "invalid_response_page_arg"
    );

    let zero_page_args = serde_json::Map::from_iter([
        ("action".to_owned(), json!("status")),
        ("_response_page_bytes".to_owned(), json!(0)),
    ]);
    let zero_page = response_page_request(Some(&zero_page_args)).unwrap_err();
    assert_eq!(
        zero_page.data.unwrap()["code"],
        "invalid_response_page_bytes"
    );
}
