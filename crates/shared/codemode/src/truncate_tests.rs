use serde_json::json;

use super::truncate::{redact_secret_like_segments, truncate_execution_response};
use super::types::CodeModeExecutionResponse;

#[test]
fn redacts_secret_like_segments() {
    assert_eq!(
        redact_secret_like_segments("hello redaction-canary-abcdefghijklmnopqrstuvwxyz"),
        "hello [REDACTED]"
    );
}

#[test]
fn truncates_large_response_result() {
    let response = CodeModeExecutionResponse {
        result: Some(json!({"body": "x".repeat(2000)})),
        calls: Vec::new(),
        logs: Vec::new(),
        error: None,
        ui: None,
    };
    let truncated = truncate_execution_response(response, 512, 512, 1);
    assert_eq!(truncated.result.unwrap()["truncated"], true);
}
