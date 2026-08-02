use serde_json::Value;
use soma_ops::{DiagnosticCode, OperationName};

use crate::{CompatibilityError, DiagnosticProjection, LegacyPresentation, SynapseCatalog};

/// Result projected for a legacy Flux or Scout caller.
#[derive(Debug, Clone, PartialEq)]
pub enum LegacyProjectedResult {
    /// Structured canonical JSON payload.
    Json(Value),
    /// Deterministic human-readable Markdown.
    Markdown(String),
}

impl SynapseCatalog {
    /// Validates a canonical result payload.
    pub fn validate_result(
        &self,
        operation: &OperationName,
        result: &Value,
    ) -> Result<(), CompatibilityError> {
        self.result_schema(operation)
            .ok_or_else(|| CompatibilityError::UnknownOperation(operation.clone()))?
            .validate(operation, "result", result)
    }

    /// Projects a canonical result payload to the requested legacy presentation.
    pub fn project_result(
        &self,
        operation: &OperationName,
        result: &Value,
        presentation: LegacyPresentation,
    ) -> Result<LegacyProjectedResult, CompatibilityError> {
        let contract = self
            .result_schema(operation)
            .ok_or_else(|| CompatibilityError::UnknownOperation(operation.clone()))?;
        contract.validate(operation, "result", result)?;
        match presentation {
            LegacyPresentation::Json => Ok(LegacyProjectedResult::Json(result.clone())),
            LegacyPresentation::Markdown => Ok(LegacyProjectedResult::Markdown(render_markdown(
                operation,
                contract.family().unwrap_or("unknown"),
                result,
            ))),
        }
    }

    /// Returns a diagnostic projection only when declared by the operation.
    pub fn project_diagnostic(
        &self,
        operation: &OperationName,
        code: &DiagnosticCode,
    ) -> Result<&DiagnosticProjection, CompatibilityError> {
        let spec = self
            .operation(operation)
            .ok_or_else(|| CompatibilityError::UnknownOperation(operation.clone()))?;
        if !spec.allows_diagnostic(code) {
            return Err(CompatibilityError::DiagnosticNotDeclared {
                operation: operation.clone(),
                code: code.clone(),
            });
        }
        self.diagnostic_projection(code)
            .ok_or_else(|| CompatibilityError::UnknownDiagnostic(code.clone()))
    }
}

fn render_markdown(operation: &OperationName, family: &str, result: &Value) -> String {
    match family {
        "mutation" => result
            .get("summary")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| fenced_json(operation, result)),
        "status" => result
            .get("status")
            .and_then(Value::as_str)
            .map(|status| format!("**Status:** {status}"))
            .unwrap_or_else(|| fenced_json(operation, result)),
        "text" | "file_content" => inline_or_artifact(result, "content"),
        "command" => render_command(result),
        "diff" => result
            .get("patch")
            .and_then(Value::as_str)
            .map(|patch| format!("```diff\n{patch}\n```"))
            .unwrap_or_else(|| inline_or_artifact(result, "patch")),
        _ => fenced_json(operation, result),
    }
}

fn render_command(result: &Value) -> String {
    let exit = result
        .get("exit_code")
        .and_then(Value::as_i64)
        .unwrap_or(-1);
    let stdout = result.get("stdout").and_then(Value::as_str).unwrap_or("");
    let stderr = result.get("stderr").and_then(Value::as_str).unwrap_or("");
    if stdout.is_empty() && stderr.is_empty() {
        return format!("**Exit code:** {exit}");
    }
    let mut output = format!("**Exit code:** {exit}\n");
    if !stdout.is_empty() {
        output.push_str("\n```text\n");
        output.push_str(stdout);
        output.push_str("\n```\n");
    }
    if !stderr.is_empty() {
        output.push_str("\n**stderr**\n\n```text\n");
        output.push_str(stderr);
        output.push_str("\n```\n");
    }
    output
}

fn inline_or_artifact(result: &Value, field: &str) -> String {
    if let Some(value) = result.get(field).and_then(Value::as_str) {
        return value.to_owned();
    }
    let artifact = format!("{field}_artifact");
    result
        .get(&artifact)
        .and_then(|value| value.get("uri"))
        .and_then(Value::as_str)
        .map(|uri| format!("Protected artifact: `{uri}`"))
        .unwrap_or_default()
}

fn fenced_json(operation: &OperationName, result: &Value) -> String {
    let body = serde_json::to_string_pretty(result).unwrap_or_else(|_| "{}".into());
    format!("### {operation}\n\n```json\n{body}\n```")
}

#[cfg(test)]
#[path = "projection_tests.rs"]
mod tests;
