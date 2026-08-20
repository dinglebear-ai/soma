use serde_json::json;

use super::truncate::{
    redact_secret_like_segments, response_within_budget, truncate_execution_response,
};
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
        result: Some(json!({"body": "x".repeat(4000)})),
        calls: Vec::new(),
        logs: Vec::new(),
        error: None,
        ui: None,
    };
    let truncated = truncate_execution_response(response, 1536, 1536, 1);
    let result = truncated.result.as_ref().expect("truncation marker");
    assert_eq!(result["truncated"], true);
    assert!(result["next_action"].as_str().is_some());
    assert!(response_within_budget(&truncated, 1536, 1536, 1));
}

#[test]
fn result_marker_adapts_to_minimum_valid_byte_budget() {
    let response = CodeModeExecutionResponse {
        result: Some(json!({"body": "x".repeat(10_000)})),
        calls: Vec::new(),
        logs: Vec::new(),
        error: None,
        ui: None,
    };

    let truncated = truncate_execution_response(response, 1024, 100_000, 4);
    let marker = truncated.result.as_ref().expect("truncation marker");

    assert_eq!(marker["truncated"], true);
    assert!(
        marker["preview"]
            .as_str()
            .is_some_and(|preview| !preview.is_empty())
    );
    assert!(response_within_budget(&truncated, 1024, 100_000, 4));
}

#[test]
fn tight_token_budget_falls_back_to_compact_marker() {
    let response = CodeModeExecutionResponse {
        result: Some(json!({"body": "x".repeat(10_000)})),
        calls: Vec::new(),
        logs: Vec::new(),
        error: None,
        ui: None,
    };

    let truncated = truncate_execution_response(response, 4096, 128, 1);
    let marker = truncated.result.as_ref().expect("truncation marker");

    assert_eq!(marker["truncated"], true);
    assert!(marker.get("preview").is_none());
    assert!(response_within_budget(&truncated, 4096, 128, 1));
}

#[test]
fn result_marker_preserves_utf8_boundaries_when_preview_is_reduced() {
    let response = CodeModeExecutionResponse {
        result: Some(json!({"body": "🦀".repeat(3000)})),
        calls: Vec::new(),
        logs: Vec::new(),
        error: None,
        ui: None,
    };

    let truncated = truncate_execution_response(response, 1024, 100_000, 4);
    let preview = truncated
        .result
        .as_ref()
        .and_then(|marker| marker["preview"].as_str())
        .expect("UTF-8-safe preview");

    assert!(std::str::from_utf8(preview.as_bytes()).is_ok());
    assert!(response_within_budget(&truncated, 1024, 100_000, 4));
}

#[test]
fn log_dominant_response_preserves_small_result_and_newest_logs() {
    let result = json!({"ok": true});
    let newest = "newest-context".repeat(5);
    let response = CodeModeExecutionResponse {
        result: Some(result.clone()),
        calls: Vec::new(),
        logs: vec![
            "old-context".repeat(80),
            "middle-context".repeat(60),
            newest.clone(),
        ],
        error: None,
        ui: None,
    };

    let truncated = truncate_execution_response(response, 512, 512, 1);

    assert_eq!(truncated.result, Some(result));
    assert!(
        truncated
            .logs
            .first()
            .is_some_and(|line| line.starts_with("[logs truncated to fit response budget:"))
    );
    assert_eq!(truncated.logs.last(), Some(&newest));
    assert!(response_within_budget(&truncated, 512, 512, 1));
}

#[test]
fn log_budget_accounts_for_json_escaping() {
    let newest = "newest-\"quoted\"-context".repeat(4);
    let response = CodeModeExecutionResponse {
        result: Some(json!(1)),
        calls: Vec::new(),
        logs: vec![
            "old-\\-context".repeat(100),
            "middle-\"context\"".repeat(70),
            newest.clone(),
        ],
        error: None,
        ui: None,
    };

    let truncated = truncate_execution_response(response, 480, 480, 1);

    assert_eq!(truncated.logs.last(), Some(&newest));
    assert!(response_within_budget(&truncated, 480, 480, 1));
}

#[test]
fn log_sentinel_is_omitted_when_even_it_cannot_fit() {
    let response = CodeModeExecutionResponse {
        result: None,
        calls: Vec::new(),
        logs: vec!["x".repeat(1000)],
        error: None,
        ui: None,
    };

    let truncated = truncate_execution_response(response, 32, 32, 1);

    assert!(truncated.logs.is_empty());
    assert!(response_within_budget(&truncated, 32, 32, 1));
}
