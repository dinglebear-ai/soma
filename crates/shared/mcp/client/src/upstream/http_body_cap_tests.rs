use std::collections::HashMap;

use http::{HeaderName, HeaderValue};
use rmcp::model::ClientJsonRpcMessage;
use rmcp::transport::common::http_header::{HEADER_MCP_METHOD, HEADER_MCP_NAME};

use super::{
    CappedStreamError, account_event_bytes, apply_message_headers, extract_scope,
    jsonrpc_name_header, validate_custom_header,
};

#[test]
fn reserved_headers_reject_client_overrides() {
    assert!(validate_custom_header(&HeaderName::from_static("accept")).is_err());
    assert!(validate_custom_header(&HeaderName::from_static("mcp-session-id")).is_err());
    assert!(validate_custom_header(&HeaderName::from_static("last-event-id")).is_err());
    assert!(validate_custom_header(&HeaderName::from_static("x-safe")).is_ok());
}

#[test]
fn protocol_version_header_is_allowed_for_worker_injection() {
    assert!(validate_custom_header(&HeaderName::from_static("mcp-protocol-version")).is_ok());
}

#[test]
fn message_headers_replace_stale_method_and_name_but_preserve_param_headers() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let message: ClientJsonRpcMessage = serde_json::from_value(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "header.tool",
            "arguments": {"owner": "alice"}
        }
    }))
    .expect("valid client JSON-RPC request");
    let mut headers = HashMap::from([
        (
            HeaderName::from_static("mcp-method"),
            HeaderValue::from_static("stale/method"),
        ),
        (
            HeaderName::from_static("mcp-name"),
            HeaderValue::from_static("stale.name"),
        ),
        (
            HeaderName::from_static("mcp-param-owner"),
            HeaderValue::from_static("alice"),
        ),
    ]);
    let request = apply_message_headers(
        reqwest::Client::new().post("http://example.invalid/mcp"),
        &message,
        &mut headers,
    )
    .expect("headers apply")
    .build()
    .expect("request builds");

    assert_eq!(
        request.headers().get_all(HEADER_MCP_METHOD).iter().count(),
        1
    );
    assert_eq!(request.headers()[HEADER_MCP_METHOD], "tools/call");
    assert_eq!(request.headers().get_all(HEADER_MCP_NAME).iter().count(), 1);
    assert_eq!(request.headers()[HEADER_MCP_NAME], "header.tool");
    assert_eq!(request.headers()["mcp-param-owner"], "alice");
}

#[test]
fn name_header_base64_wraps_non_ascii_values() {
    let message: ClientJsonRpcMessage = serde_json::from_value(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": "café", "arguments": {}}
    }))
    .expect("valid client JSON-RPC request");
    let name = jsonrpc_name_header(&message).expect("name header");
    let encoded = name.to_str().expect("ASCII header value");
    assert!(encoded.starts_with("=?base64?"));
    assert!(encoded.ends_with("?="));
}

#[test]
fn scope_extraction_accepts_quoted_and_bare_forms() {
    assert_eq!(
        extract_scope(r#"Bearer error="insufficient_scope", scope="files:read files:write""#),
        Some("files:read files:write".to_owned())
    );
    assert_eq!(
        extract_scope(r#"Bearer scope=files:read,error="insufficient_scope""#),
        Some("files:read".to_owned())
    );
}

#[test]
fn sse_event_counter_resets_on_boundaries() {
    let mut state = (0usize, false);
    account_event_bytes(b"data: one\n\n", &mut state, 10).expect("first event under cap");
    assert_eq!(state.0, 0);
    account_event_bytes(b"data: two", &mut state, 10).expect("second event under cap");
    assert_eq!(state.0, 9);
}

#[test]
fn sse_event_counter_detects_cross_chunk_boundaries() {
    let mut state = (0usize, false);
    account_event_bytes(b"data: a\n", &mut state, 8).expect("partial event under cap");
    account_event_bytes(b"\ndata: b", &mut state, 8).expect("boundary resets next event");
    assert_eq!(state.0, 7);
}

#[test]
fn sse_event_counter_rejects_single_oversized_event() {
    let mut state = (0usize, false);
    let error = account_event_bytes(b"data: too-big", &mut state, 6)
        .expect_err("single event should exceed cap");
    assert!(matches!(error, CappedStreamError::TooLarge { .. }));
    assert!(error.to_string().contains("response_too_large"));
}
