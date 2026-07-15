use crate::types::CodeModeExecutedCall;

pub fn artifact_call(path: &str) -> CodeModeExecutedCall {
    CodeModeExecutedCall {
        id: "artifact::write".to_string(),
        params: Some(serde_json::json!({"path": path})),
        result: None,
    }
}
