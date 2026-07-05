use serde::Serialize;
use serde_json::{json, Value};

use crate::actions::{action_names, ActionValidationError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceErrorKind {
    Validation,
    Timeout,
    RateLimited,
    AuthRejected,
    UpstreamUnavailable,
    Execution,
    Internal,
}

impl ServiceErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Timeout => "timeout",
            Self::RateLimited => "rate_limited",
            Self::AuthRejected => "auth_rejected",
            Self::UpstreamUnavailable => "upstream_unavailable",
            Self::Execution => "execution",
            Self::Internal => "internal",
        }
    }

    pub fn http_status_code(self) -> u16 {
        match self {
            Self::Validation => 400,
            Self::AuthRejected => 403,
            Self::RateLimited => 429,
            Self::Timeout | Self::UpstreamUnavailable => 503,
            Self::Execution | Self::Internal => 500,
        }
    }

    pub fn retryable(self) -> bool {
        matches!(
            self,
            Self::Validation | Self::Timeout | Self::RateLimited | Self::UpstreamUnavailable
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolError {
    pub schema_version: u8,
    pub kind: ServiceErrorKind,
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub remediation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bad_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_kind: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub available_actions: Vec<&'static str>,
}

impl ToolError {
    pub fn validation(
        code: impl Into<String>,
        message: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: 1,
            kind: ServiceErrorKind::Validation,
            code: code.into(),
            message: message.into(),
            retryable: true,
            remediation: remediation.into(),
            field: None,
            bad_value: None,
            expected_pattern: None,
            reason_kind: None,
            available_actions: Vec::new(),
        }
    }

    pub fn from_action_validation(error: &ActionValidationError) -> Self {
        Self::from_action_validation_with_actions(error, action_names())
    }

    pub fn from_action_validation_with_actions(
        error: &ActionValidationError,
        available_actions: Vec<&'static str>,
    ) -> Self {
        let mut tool_error = Self::validation(error.code(), error.to_string(), error.remediation())
            .with_available_actions(available_actions);
        if let Some(field) = error.field() {
            tool_error = tool_error.with_field(field);
        }
        if let Some(bad_value) = error.bad_value() {
            tool_error = tool_error.with_bad_value(bad_value);
        }
        tool_error
    }

    pub fn execution(error: &anyhow::Error) -> Self {
        let reason_kind = classify_execution_error(error);
        Self {
            schema_version: 1,
            kind: reason_kind,
            code: "execution_error".to_owned(),
            message: "Tool execution failed. Check server logs for details.".to_owned(),
            retryable: reason_kind.retryable(),
            remediation: "Check service configuration and upstream availability, then retry. Use action=status or action=help for diagnostics.".to_owned(),
            field: None,
            bad_value: None,
            expected_pattern: None,
            reason_kind: Some(reason_kind.as_str().to_owned()),
            available_actions: Vec::new(),
        }
    }

    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }

    pub fn with_bad_value(mut self, bad_value: impl Into<String>) -> Self {
        self.bad_value = Some(bad_value.into());
        self
    }

    pub fn with_expected_pattern(mut self, expected_pattern: impl Into<String>) -> Self {
        self.expected_pattern = Some(expected_pattern.into());
        self
    }

    pub fn with_available_actions(mut self, available_actions: Vec<&'static str>) -> Self {
        self.available_actions = available_actions;
        self
    }

    pub fn http_status_code(&self) -> u16 {
        self.kind.http_status_code()
    }

    pub fn to_rest_payload(&self) -> Value {
        let mut payload = json!({
            "error": self.message,
            "kind": self.kind.as_str(),
            "schema_version": self.schema_version,
            "code": self.code,
            "message": self.message,
            "retryable": self.retryable,
            "remediation": self.remediation,
        });
        self.add_optional_fields(&mut payload);
        payload
    }

    pub fn to_mcp_payload(&self, tool: &str, action: Option<&str>) -> Value {
        let mut payload = json!({
            "kind": "mcp_tool_error",
            "schema_version": self.schema_version,
            "code": self.code,
            "tool": tool,
            "action": action,
            "message": self.message,
            "retryable": self.retryable,
            "remediation": self.remediation,
        });
        payload["service_error_kind"] = json!(self.kind.as_str());
        self.add_optional_fields(&mut payload);
        payload
    }

    fn add_optional_fields(&self, payload: &mut Value) {
        if let Some(field) = &self.field {
            payload["field"] = json!(field);
        }
        if let Some(bad_value) = &self.bad_value {
            payload["bad_value"] = json!(bad_value);
        }
        if let Some(expected_pattern) = &self.expected_pattern {
            payload["expected_pattern"] = json!(expected_pattern);
        }
        if let Some(reason_kind) = &self.reason_kind {
            payload["reason_kind"] = json!(reason_kind);
        }
        if !self.available_actions.is_empty() {
            payload["available_actions"] = json!(self.available_actions);
        }
    }
}

pub type ServiceError = ToolError;

pub fn classify_execution_error(error: &anyhow::Error) -> ServiceErrorKind {
    let text = error.to_string().to_ascii_lowercase();
    if text.contains("timeout") || text.contains("timed out") {
        ServiceErrorKind::Timeout
    } else if text.contains("rate limit") || text.contains("429") {
        ServiceErrorKind::RateLimited
    } else if text.contains("unauthorized")
        || text.contains("forbidden")
        || text.contains("401")
        || text.contains("403")
    {
        ServiceErrorKind::AuthRejected
    } else if text.contains("connection refused")
        || text.contains("connection reset")
        || text.contains("dns")
        || text.contains("unreachable")
        || text.contains("temporarily unavailable")
    {
        ServiceErrorKind::UpstreamUnavailable
    } else {
        ServiceErrorKind::Execution
    }
}

#[cfg(test)]
#[path = "errors_tests.rs"]
mod tests;
