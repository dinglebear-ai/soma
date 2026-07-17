//! Implements `soma-application`'s [`CodeModePort`] over `soma-codemode`'s
//! sandboxed JS snippet runner (plan section 3.20, "CodeModeExecutor" in
//! section 5's illustrative flow).
//!
//! There is exactly one Code Mode execution engine in the workspace
//! (`soma_codemode::execute::execute_inline`, which spawns the bounded
//! `soma-codemode-runner` subprocess); this adapter calls it directly rather
//! than re-implementing any part of the runner, sandbox, or result-shaping
//! pipeline — the same engine `soma-provider-adapters::codemode` bridges to
//! for drop-in providers.
//!
//! `CodeModeExecuteRequest::input` is not yet threaded into the snippet: the
//! runner's `Start` protocol message has no side-channel for caller-supplied
//! input today. This mirrors the identical, already-documented limitation on
//! `soma_provider_adapters::codemode::CodeModeSnippetProvider`, not a new gap
//! introduced here.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};

use soma_application::{CodeModeExecuteRequest, CodeModePort, ExecutionContext, PortError};
use soma_codemode::{execute::execute_inline, CodeModeConfig, UiLink};

#[derive(Clone, Default)]
pub struct CodeModeApplicationPort {
    config: CodeModeConfig,
}

impl CodeModeApplicationPort {
    pub fn new(config: CodeModeConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl CodeModePort for CodeModeApplicationPort {
    async fn execute(
        &self,
        request: CodeModeExecuteRequest,
        _context: &ExecutionContext,
    ) -> Result<Value, PortError> {
        let ui_capture: Arc<Mutex<Option<UiLink>>> = Arc::new(Mutex::new(None));
        let outcome = execute_inline(&request.source, self.config.clone(), ui_capture)
            .await
            .map_err(|error| {
                let mut port_error = PortError::new("codemode_execution_failed", error.to_string());
                port_error.remediation =
                    "Check the Code Mode snippet and runner configuration, then retry.".to_owned();
                port_error
            })?;
        Ok(json!({
            "result": outcome.display_response.result,
            "logs": outcome.display_response.logs,
        }))
    }
}

#[cfg(test)]
#[path = "codemode_tests.rs"]
mod tests;
