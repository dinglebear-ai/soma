use serde_json::json;

use super::trace::{code_mode_execute_trace, redact_trace_value};
use super::types::{CodeModeExecutedCall, CodeModeExecutionResponse};

#[test]
fn trace_redacts_sensitive_values() {
    let value = redact_trace_value(&json!({"token": "secret", "safe": true}), 4096);
    assert_eq!(value["token"], "[redacted]");
    assert_eq!(value["safe"], true);
}

#[test]
fn execute_trace_includes_call_count() {
    let response = CodeModeExecutionResponse {
        result: Some(json!({"api_key": "redaction-canary-abcdefghijklmnopqrst"})),
        calls: vec![CodeModeExecutedCall {
            id: "demo::call".to_string(),
            params: Some(json!({
                "token": "redaction-canary-bcdefghijklmnopqrstu",
                "visible": true
            })),
            result: Some(json!({"secret": "redaction-canary-cdefghijklmnopqrstuv"})),
        }],
        logs: Vec::new(),
        error: None,
        ui: None,
    };
    let trace = code_mode_execute_trace(&response);
    assert_eq!(trace["call_count"], 1);
    assert_eq!(trace["calls"][0]["params"]["token"], "[redacted]");
    assert_eq!(trace["calls"][0]["params"]["visible"], true);
    assert_eq!(trace["result"]["api_key"], "[redacted]");
    assert!(!trace.to_string().contains("redaction-canary-"));
}
