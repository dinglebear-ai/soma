use std::sync::Arc;

use crate::protocol::CodeModeRunnerResult;
use crate::types::{CodeModeExecutionResponse, UiLink};

pub fn response_from_runner(
    result: CodeModeRunnerResult,
    ui_capture: Arc<std::sync::Mutex<Option<UiLink>>>,
) -> CodeModeExecutionResponse {
    CodeModeExecutionResponse {
        result: result.into_response_result(),
        calls: Vec::new(),
        logs: Vec::new(),
        error: None,
        ui: ui_capture.lock().ok().and_then(|guard| guard.clone()),
    }
}
