use serde_json::{json, Value};

pub const GATEWAY_ERROR_SCHEMA_VERSION: &str = "mcp.gateway.error.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayStructuredError {
    pub code: &'static str,
    pub kind: &'static str,
    pub tool: &'static str,
    pub action: String,
    pub remediation: &'static str,
}

impl GatewayStructuredError {
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "isError": true,
            "schema_version": GATEWAY_ERROR_SCHEMA_VERSION,
            "code": self.code,
            "kind": self.kind,
            "tool": self.tool,
            "action": self.action,
            "remediation": self.remediation,
        })
    }
}

pub fn structured_error(
    action: impl Into<String>,
    code: &'static str,
    kind: &'static str,
    remediation: &'static str,
) -> GatewayStructuredError {
    GatewayStructuredError {
        code,
        kind,
        tool: "gateway",
        action: action.into(),
        remediation,
    }
}

#[cfg(test)]
#[path = "dispatch_helpers_tests.rs"]
mod tests;
