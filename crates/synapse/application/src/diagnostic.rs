use serde::Deserialize;
use soma_ops::{DiagnosticCode, DiagnosticSeverity, RetryClass};

/// Product-owned projection of a stable diagnostic to every legacy surface.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DiagnosticProjection {
    code: DiagnosticCode,
    category: String,
    cli_exit_code: u8,
    http_status: u16,
    mcp_error_code: i32,
    event_severity: DiagnosticSeverity,
    retry: RetryClass,
    terminal: bool,
}

impl DiagnosticProjection {
    /// Returns the stable code.
    #[must_use]
    pub fn code(&self) -> &DiagnosticCode {
        &self.code
    }
    /// Returns the diagnostic category.
    #[must_use]
    pub fn category(&self) -> &str {
        &self.category
    }
    /// Returns the CLI process exit code.
    #[must_use]
    pub const fn cli_exit_code(&self) -> u8 {
        self.cli_exit_code
    }
    /// Returns the REST HTTP status.
    #[must_use]
    pub const fn http_status(&self) -> u16 {
        self.http_status
    }
    /// Returns the JSON-RPC/MCP error code, or zero for non-errors.
    #[must_use]
    pub const fn mcp_error_code(&self) -> i32 {
        self.mcp_error_code
    }
    /// Returns lifecycle event severity.
    #[must_use]
    pub const fn event_severity(&self) -> DiagnosticSeverity {
        self.event_severity
    }
    /// Returns retry classification.
    #[must_use]
    pub const fn retry(&self) -> RetryClass {
        self.retry
    }
    /// Returns whether the diagnostic terminates the operation.
    #[must_use]
    pub const fn terminal(&self) -> bool {
        self.terminal
    }
}

#[cfg(test)]
#[path = "diagnostic_tests.rs"]
mod tests;
