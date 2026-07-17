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
//!
//! `CodeModeConfig::enabled` (default `false`) is checked explicitly by this
//! adapter before delegating to `execute_inline`, unlike
//! `soma_provider_adapters::codemode::CodeModeSnippetProvider` and
//! `execute_inline` itself, neither of which consult the flag. No action,
//! CLI command, or REST route dispatches to `codemode_execute` yet (see
//! `crates/soma/domain/src/actions.rs`), so this check has no observable
//! effect today; it exists so a future PR that wires a live surface to this
//! port gets a clear `codemode_disabled` error instead of silently running
//! snippets through a config the operator marked disabled.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};

use soma_application::{CodeModeExecuteRequest, CodeModePort, ExecutionContext, PortError};
use soma_codemode::{execute::execute_inline, CodeModeConfig, ToolError, UiLink};

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
        if !self.config.enabled {
            let mut port_error = PortError::new(
                "codemode_disabled",
                "Code Mode is disabled for this instance",
            );
            port_error.remediation =
                "Enable Code Mode in the runtime configuration and retry.".to_owned();
            return Err(port_error);
        }
        let ui_capture: Arc<Mutex<Option<UiLink>>> = Arc::new(Mutex::new(None));
        let outcome = execute_inline(&request.source, self.config.clone(), ui_capture)
            .await
            .map_err(codemode_port_error)?;
        Ok(json!({
            "result": outcome.display_response.result,
            "logs": outcome.display_response.logs,
        }))
    }
}

/// Maps `soma-codemode`'s `ToolError` taxonomy onto a `PortError`, preserving
/// the distinction between caller mistakes (invalid/missing params, unknown
/// action), authorization failures, and runner/SDK failures, instead of
/// collapsing every case into one generic code and remediation string.
fn codemode_port_error(error: ToolError) -> PortError {
    let (retryable, remediation): (bool, String) = match &error {
        ToolError::MissingParam { .. } | ToolError::InvalidParam { .. } => (
            false,
            "Fix the Code Mode snippet or its parameters and retry.".to_owned(),
        ),
        ToolError::UnknownAction { .. } | ToolError::UnknownInstance { .. } => (
            false,
            "Check the referenced action or instance name and retry.".to_owned(),
        ),
        ToolError::AmbiguousTool { .. } => (
            false,
            "Disambiguate the referenced tool and retry.".to_owned(),
        ),
        ToolError::Forbidden {
            required_scopes, ..
        } => (
            false,
            format!(
                "Request the required scope(s) and retry: {}",
                required_scopes.join(", ")
            ),
        ),
        ToolError::Conflict { .. } => (
            false,
            "Resolve the conflicting resource and retry.".to_owned(),
        ),
        ToolError::ConfirmationRequired { .. } => {
            (false, "Re-run with explicit confirmation.".to_owned())
        }
        ToolError::Sdk { .. } => (
            true,
            "Retry; if this persists, check the Code Mode runner's health.".to_owned(),
        ),
    };
    let code = format!("codemode_{}", error.kind());
    let mut port_error = PortError::new(code, error.to_string());
    port_error.retryable = retryable;
    port_error.remediation = remediation;
    port_error
}

#[cfg(test)]
#[path = "codemode_tests.rs"]
mod tests;
