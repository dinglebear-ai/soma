use serde_json::Value;

use crate::types::CodeModeExecutedCall;

pub fn executed_call(
    id: impl Into<String>,
    params: Option<Value>,
    result: Option<Value>,
) -> CodeModeExecutedCall {
    CodeModeExecutedCall {
        id: id.into(),
        params,
        result,
    }
}
