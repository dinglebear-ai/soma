use std::sync::Arc;

use crate::execute::{
    runner::{execute_in_subprocess, SubprocessExecution},
    CodeModeExecutionOutcome,
};
use crate::host::CodeModeHost;
use crate::types::{CodeModeCaller, CodeModeExecutionResponse, CodeModeSurface, ToolScope, UiLink};
use crate::{CodeModeConfig, ToolError};

pub struct CodeModeBroker<'a, H: CodeModeHost> {
    pub(crate) host: Option<&'a H>,
    pub(crate) ui_capture: Arc<std::sync::Mutex<Option<UiLink>>>,
}

impl<'a, H: CodeModeHost> CodeModeBroker<'a, H> {
    #[must_use]
    pub fn new(host: Option<&'a H>) -> Self {
        Self {
            host,
            ui_capture: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub async fn execute(
        &self,
        code: &str,
        caller: CodeModeCaller,
        surface: CodeModeSurface,
        config: CodeModeConfig,
        scope: ToolScope,
        execution_id: Option<Arc<str>>,
    ) -> Result<CodeModeExecutionResponse, ToolError> {
        Ok(self
            .execute_with_raw_response(code, caller, surface, config, scope, execution_id)
            .await?
            .display_response)
    }

    pub async fn execute_with_raw_response(
        &self,
        code: &str,
        caller: CodeModeCaller,
        surface: CodeModeSurface,
        config: CodeModeConfig,
        scope: ToolScope,
        execution_id: Option<Arc<str>>,
    ) -> Result<CodeModeExecutionOutcome, ToolError> {
        execute_in_subprocess(SubprocessExecution {
            host: self.host,
            code,
            caller,
            surface,
            config,
            scope,
            execution_id,
            ui_capture: self.ui_capture.clone(),
        })
        .await
    }
}

pub fn code_mode_unknown_tool_hint() -> String {
    "Use codemode.search or codemode.describe to find an available tool id.".to_string()
}
