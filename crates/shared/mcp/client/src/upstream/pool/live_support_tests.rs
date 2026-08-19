use std::sync::Arc;

use rmcp::model::{CallToolResult, Tool, ToolAnnotations};
use serde_json::Map;

use super::{
    bearer_token_from_env, completed_tool_error, normalize_bearer_value, tool_descriptor,
    upstream_destructive_from_annotations, websocket_authorization,
};
use crate::config::UpstreamConfig;
use crate::upstream::pool::capability_is_absent;

#[test]
fn tool_descriptor_preserves_annotation_shape_and_uses_fail_closed_safety() {
    let full = Tool::new("full", "full annotations", Arc::new(Map::new())).with_annotations(
        ToolAnnotations::from_raw(
            Some("Full title".to_owned()),
            Some(true),
            None,
            Some(true),
            Some(false),
        ),
    );
    let full = tool_descriptor(full);
    assert_eq!(
        full.annotations,
        Some(serde_json::json!({
            "title": "Full title",
            "readOnlyHint": true,
            "idempotentHint": true,
            "openWorldHint": false
        }))
    );
    assert!(
        !full.destructive,
        "readOnlyHint=true must make the internal gate safe when destructiveHint is absent"
    );

    let partial =
        Tool::new("partial", "partial annotations", Arc::new(Map::new())).with_annotations(
            ToolAnnotations::from_raw(None, None, None, Some(true), None),
        );
    let partial = tool_descriptor(partial);
    assert_eq!(
        partial.annotations,
        Some(serde_json::json!({"idempotentHint": true}))
    );
    assert!(
        partial.destructive,
        "an underspecified annotation block must fail closed"
    );

    let empty = Tool::new("empty", "empty annotations", Arc::new(Map::new()))
        .with_annotations(ToolAnnotations::new());
    let empty = tool_descriptor(empty);
    assert_eq!(empty.annotations, Some(serde_json::json!({})));
    assert!(empty.destructive);

    let absent = tool_descriptor(Tool::new("absent", "no annotations", Arc::new(Map::new())));
    assert!(absent.annotations.is_none());
    assert!(absent.destructive);

    assert!(!upstream_destructive_from_annotations(Some(
        &ToolAnnotations::new().destructive(false)
    )));
    assert!(upstream_destructive_from_annotations(None));
}

#[test]
fn completed_tool_error_canonicalizes_untrusted_kinds_and_bounds_cause() {
    let valid = CallToolResult::structured_error(serde_json::json!({
        "kind": "invalid_param",
        "message": "bad field"
    }));
    assert_eq!(
        completed_tool_error(&valid),
        Some(("invalid_param".to_string(), "bad field".to_string()))
    );

    let infrastructure_claim = CallToolResult::structured_error(serde_json::json!({
        "error": {
            "kind": "server_error",
            "message": "x".repeat(2048)
        }
    }));
    let (kind, cause) = completed_tool_error(&infrastructure_claim).expect("isError result");
    assert_eq!(kind, "tool_error");
    assert_eq!(cause.chars().count(), 1024);

    assert!(
        completed_tool_error(&CallToolResult::structured(serde_json::json!({
            "kind": "invalid_param"
        })))
        .is_none()
    );
}

#[test]
fn bearer_value_normalization_accepts_raw_or_prefixed_tokens() {
    assert_eq!(normalize_bearer_value("secret"), "secret");
    assert_eq!(normalize_bearer_value(" Bearer secret "), "secret");
}

#[test]
fn bearer_token_env_supports_plain_http_and_websocket_auth() {
    let var = "SOMA_MCP_CLIENT_TEST_BEARER";
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var(var, "Bearer secret") };
    let config = UpstreamConfig {
        name: "bearer".to_owned(),
        bearer_token_env: Some(var.to_owned()),
        ..UpstreamConfig::default()
    };

    assert_eq!(bearer_token_from_env(&config).as_deref(), Some("secret"));
    assert_eq!(
        websocket_authorization(&config).as_deref(),
        Some("Bearer secret")
    );

    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var(var) };
}

#[test]
fn capability_absence_matches_json_rpc_method_not_found() {
    assert!(capability_is_absent(
        "JSON-RPC error -32601: Method not found"
    ));
    assert!(capability_is_absent("method not found"));
    assert!(!capability_is_absent("connection refused"));
}
