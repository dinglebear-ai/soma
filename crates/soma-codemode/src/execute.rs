pub mod call_tool;
pub mod discovery;
pub mod internal;
pub mod result;
pub mod runner;

#[cfg(test)]
mod call_tool_tests;
#[cfg(test)]
mod discovery_tests;
#[cfg(test)]
mod internal_tests;
#[cfg(test)]
mod result_tests;
#[cfg(test)]
mod runner_tests;

use std::sync::Arc;

use crate::shape::shape_final_result;
use crate::truncate::truncate_execution_response;
use crate::types::{CodeModeExecutionResponse, UiLink};
use crate::{normalize_user_code, CodeModeConfig, ToolError};

#[derive(Debug, Clone, PartialEq)]
pub struct CodeModeExecutionOutcome {
    pub raw_response: CodeModeExecutionResponse,
    pub display_response: CodeModeExecutionResponse,
}

pub async fn execute_inline(
    code: &str,
    config: CodeModeConfig,
    ui_capture: Arc<std::sync::Mutex<Option<UiLink>>>,
) -> Result<CodeModeExecutionOutcome, ToolError> {
    let normalized = normalize_user_code(code);
    let output =
        crate::runner::run_code_mode_runner_once(crate::protocol::CodeModeRunnerInput::Start {
            code: normalized,
            proxy: String::new(),
        })
        .map_err(ToolError::internal_message)?;
    match output {
        crate::protocol::CodeModeRunnerOutput::Done { result, .. } => {
            finish_response(result::response_from_runner(result, ui_capture), &config)
        }
        crate::protocol::CodeModeRunnerOutput::Error { kind, message } => Err(ToolError::Sdk {
            sdk_kind: kind,
            message,
        }),
        _ => Err(ToolError::internal_message(
            "runner returned an unexpected non-terminal message",
        )),
    }
}

pub(crate) fn finish_response(
    raw_response: CodeModeExecutionResponse,
    config: &CodeModeConfig,
) -> Result<CodeModeExecutionOutcome, ToolError> {
    let shaped = shape_final_result(
        raw_response.result.clone(),
        config.result_shape_policy,
        config.max_response_bytes,
        config.max_response_tokens,
        config.token_estimate_divisor,
    );
    let mut display_response = raw_response.clone();
    display_response.result = shaped.result;
    display_response = truncate_execution_response(
        display_response,
        config.max_response_bytes,
        config.max_response_tokens,
        config.token_estimate_divisor,
    );
    Ok(CodeModeExecutionOutcome {
        raw_response,
        display_response,
    })
}
