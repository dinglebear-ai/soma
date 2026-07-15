use serde_json::json;

use super::error::ToolError;

#[test]
fn tool_error_serializes_stable_kind() {
    let error = ToolError::MissingParam {
        message: "missing name".to_string(),
        param: "name".to_string(),
    };
    assert_eq!(error.kind(), "missing_param");
    assert_eq!(
        serde_json::to_value(&error).unwrap()["kind"],
        "missing_param"
    );
    assert!(error.to_string().contains("\"param\":\"name\""));
}

#[test]
fn internal_message_is_structured() {
    let error = ToolError::internal_message("boom");
    assert_eq!(
        serde_json::to_value(error).unwrap(),
        json!({"kind": "internal_error", "message": "boom"})
    );
}
