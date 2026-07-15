use serde_json::json;

use super::response::{CodeModeExecutedCall, CodeModeExecutionResponse};

#[test]
fn response_omits_empty_vectors() {
    let response = CodeModeExecutionResponse {
        result: Some(json!(true)),
        calls: vec![CodeModeExecutedCall {
            id: "demo::ok".to_string(),
            params: None,
            result: Some(json!(true)),
        }],
        logs: Vec::new(),
        error: None,
        ui: None,
    };
    let value = serde_json::to_value(response).unwrap();
    assert!(value.get("logs").is_none());
    assert_eq!(value["calls"][0]["id"], "demo::ok");
}
