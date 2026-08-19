//! Neutral analysis of completed MCP tool-execution failures.

use rmcp::model::CallToolResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const TOOL_EXECUTION_CONTRACT_VERSION: u32 = 1;
const MAX_CAUSE_CHARS: usize = 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSafetyHints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

impl ToolSafetyHints {
    #[must_use]
    pub fn from_annotations(annotations: Option<&Value>) -> Self {
        let object = annotations.and_then(Value::as_object);
        Self {
            read_only_hint: object
                .and_then(|value| value.get("readOnlyHint"))
                .and_then(Value::as_bool),
            destructive_hint: object
                .and_then(|value| value.get("destructiveHint"))
                .and_then(Value::as_bool),
            idempotent_hint: object
                .and_then(|value| value.get("idempotentHint"))
                .and_then(Value::as_bool),
            open_world_hint: object
                .and_then(|value| value.get("openWorldHint"))
                .and_then(Value::as_bool),
        }
    }

    #[must_use]
    pub fn exact_retry_is_hint_safe(&self) -> bool {
        self.read_only_hint == Some(true) || self.idempotent_hint == Some(true)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolRecoveryAction {
    ReviseAndRetry,
    RetryLater,
    Reauthenticate,
    Confirm,
    Rediscover,
    ReduceWork,
    InspectAndEscalate,
    DoNotRetry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SameArgumentsRetry {
    Conditional,
    Discouraged,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSideEffectRisk {
    NoneExpected,
    Possible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRecoveryAdvice {
    pub action: ToolRecoveryAction,
    pub same_arguments: SameArgumentsRetry,
    pub guidance: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolExecutionAnalysis {
    pub contract_version: u32,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_kind: Option<String>,
    pub cause: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    pub safety: ToolSafetyHints,
    pub recovery: ToolRecoveryAdvice,
    pub side_effects: ToolSideEffectRisk,
}

impl ToolExecutionAnalysis {
    #[must_use]
    pub fn retryable(&self) -> bool {
        matches!(self.recovery.action, ToolRecoveryAction::RetryLater)
    }
}

impl std::fmt::Display for ToolExecutionAnalysis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.cause)
    }
}

#[must_use]
pub fn analyze_completed_tool_error(
    result: &CallToolResult,
    safety: ToolSafetyHints,
) -> Option<ToolExecutionAnalysis> {
    if result.is_error != Some(true) {
        return None;
    }
    let object = result
        .structured_content
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|object| {
            object
                .get("error")
                .and_then(Value::as_object)
                .or(Some(object))
        });
    let raw_kind = object
        .and_then(|object| object.get("kind"))
        .and_then(Value::as_str);
    let original_kind = raw_kind.and_then(safe_original_kind).map(ToOwned::to_owned);
    let kind = raw_kind
        .map(canonicalize_untrusted_upstream_kind)
        .unwrap_or("tool_error")
        .to_owned();
    let retry_after_ms = object.and_then(retry_after_ms_from_object);
    let cause = object
        .and_then(|object| {
            object
                .get("message")
                .or_else(|| object.get("cause"))
                .and_then(Value::as_str)
        })
        .unwrap_or("upstream tool reported an error")
        .chars()
        .take(MAX_CAUSE_CHARS)
        .collect();
    let recovery = recovery_for_kind(&kind, retry_after_ms, safety.exact_retry_is_hint_safe());
    let side_effects = if safety.read_only_hint == Some(true) {
        ToolSideEffectRisk::NoneExpected
    } else {
        ToolSideEffectRisk::Possible
    };
    Some(ToolExecutionAnalysis {
        contract_version: TOOL_EXECUTION_CONTRACT_VERSION,
        kind,
        original_kind,
        cause,
        retry_after_ms,
        safety,
        recovery,
        side_effects,
    })
}

fn safe_original_kind(kind: &str) -> Option<&str> {
    match kind {
        "unknown_action"
        | "unknown_subaction"
        | "missing_param"
        | "invalid_param"
        | "invalid_params"
        | "unknown_instance"
        | "confirmation_required"
        | "conflict"
        | "forbidden"
        | "unknown_tool"
        | "route_scope_denied"
        | "path_traversal"
        | "permission_denied"
        | "timeout"
        | "budget_exceeded"
        | "quota_exceeded"
        | "invalid_code_mode_id"
        | "snippet_not_found"
        | "artifact_too_large"
        | "auth_failed"
        | "oauth_needs_reauth"
        | "not_found"
        | "rate_limited"
        | "validation_failed"
        | "code_mode_timeout"
        | "server_error"
        | "internal_error"
        | "runtime_error"
        | "transport_error" => Some(kind),
        _ => None,
    }
}

#[must_use]
pub fn canonicalize_untrusted_upstream_kind(kind: &str) -> &'static str {
    match kind {
        "unknown_action" => "unknown_action",
        "unknown_subaction" => "unknown_subaction",
        "missing_param" => "missing_param",
        "invalid_param" | "invalid_params" => "invalid_param",
        "unknown_instance" => "unknown_instance",
        "confirmation_required" => "confirmation_required",
        "conflict" => "conflict",
        "forbidden" => "forbidden",
        "unknown_tool" => "unknown_tool",
        "route_scope_denied" => "route_scope_denied",
        "path_traversal" => "path_traversal",
        "permission_denied" => "permission_denied",
        "timeout" => "timeout",
        "budget_exceeded" => "budget_exceeded",
        "quota_exceeded" => "quota_exceeded",
        "invalid_code_mode_id" => "invalid_code_mode_id",
        "snippet_not_found" => "snippet_not_found",
        "artifact_too_large" => "artifact_too_large",
        "auth_failed" => "auth_failed",
        "oauth_needs_reauth" => "oauth_needs_reauth",
        "not_found" => "not_found",
        "rate_limited" => "rate_limited",
        "validation_failed" => "validation_failed",
        "code_mode_timeout" => "code_mode_timeout",
        _ => "tool_error",
    }
}

fn retry_after_ms_from_object(object: &serde_json::Map<String, Value>) -> Option<u64> {
    object
        .get("retry_after_ms")
        .or_else(|| object.get("retryAfterMs"))
        .and_then(Value::as_u64)
}

fn recovery_for_kind(
    kind: &str,
    retry_after_ms: Option<u64>,
    exact_retry_hint_safe: bool,
) -> ToolRecoveryAdvice {
    let revised_retry = if exact_retry_hint_safe {
        SameArgumentsRetry::Conditional
    } else {
        SameArgumentsRetry::Discouraged
    };
    let advice = match kind {
        "missing_param" | "invalid_param" | "validation_failed" | "conflict" | "tool_error" => (
            ToolRecoveryAction::ReviseAndRetry,
            revised_retry,
            "Inspect the error details, correct the command or parameters, and retry only after changing the call.",
            None,
        ),
        "unknown_action"
        | "unknown_subaction"
        | "unknown_tool"
        | "unknown_instance"
        | "not_found"
        | "snippet_not_found"
        | "invalid_code_mode_id" => (
            ToolRecoveryAction::Rediscover,
            SameArgumentsRetry::Never,
            "List or search the available tools or resources, then retry with a valid identifier.",
            None,
        ),
        "rate_limited" | "timeout" | "code_mode_timeout" => (
            ToolRecoveryAction::RetryLater,
            SameArgumentsRetry::Conditional,
            "Wait for the dependency or limit to recover, verify possible partial effects, then retry when appropriate.",
            retry_after_ms,
        ),
        "auth_failed" | "oauth_needs_reauth" => (
            ToolRecoveryAction::Reauthenticate,
            SameArgumentsRetry::Never,
            "Repair or refresh authentication before retrying.",
            None,
        ),
        "confirmation_required" => (
            ToolRecoveryAction::Confirm,
            SameArgumentsRetry::Never,
            "Obtain explicit user confirmation and retry through the confirmed path.",
            None,
        ),
        "budget_exceeded" | "quota_exceeded" | "artifact_too_large" => (
            ToolRecoveryAction::ReduceWork,
            SameArgumentsRetry::Never,
            "Reduce fan-out or payload size, split the work, or use an artifact before retrying.",
            None,
        ),
        "forbidden" | "permission_denied" | "route_scope_denied" | "path_traversal" => (
            ToolRecoveryAction::DoNotRetry,
            SameArgumentsRetry::Never,
            "The caller lacks permission for this operation. Use an authorized route or request access.",
            None,
        ),
        _ => (
            ToolRecoveryAction::InspectAndEscalate,
            revised_retry,
            "Inspect the preserved error details, adjust the request when possible, and avoid unchanged retries when side effects are uncertain.",
            retry_after_ms,
        ),
    };
    ToolRecoveryAdvice {
        action: advice.0,
        same_arguments: advice.1,
        guidance: advice.2.to_owned(),
        retry_after_ms: advice.3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_error_recovery_uses_safety_hints() {
        let result = CallToolResult::structured_error(serde_json::json!({
            "kind": "invalid_param",
            "message": "bad field"
        }));
        let analysis = analyze_completed_tool_error(
            &result,
            ToolSafetyHints {
                read_only_hint: Some(true),
                ..ToolSafetyHints::default()
            },
        )
        .expect("isError result");
        assert_eq!(analysis.kind, "invalid_param");
        assert_eq!(analysis.recovery.action, ToolRecoveryAction::ReviseAndRetry);
        assert_eq!(
            analysis.recovery.same_arguments,
            SameArgumentsRetry::Conditional
        );
        assert_eq!(analysis.side_effects, ToolSideEffectRisk::NoneExpected);
    }

    #[test]
    fn arbitrary_original_kind_is_not_reflected() {
        let result = CallToolResult::structured_error(serde_json::json!({
            "kind": "secret_abcdefghijklmnopqrstuvwxyz_0123456789",
            "message": "generic failure"
        }));
        let analysis = analyze_completed_tool_error(&result, ToolSafetyHints::default())
            .expect("isError result");
        assert_eq!(analysis.kind, "tool_error");
        assert_eq!(analysis.original_kind, None);
    }

    #[test]
    fn infrastructure_kinds_are_clamped_and_cause_is_bounded() {
        let mut result = CallToolResult::structured_error(serde_json::json!({
            "error": {
                "kind": "server_error",
                "message": "x".repeat(2048),
                "retry_after_ms": 250
            }
        }));
        // Raw content may contain sensitive provider output. The public
        // analysis intentionally ignores it and only derives from the bounded
        // structured error fields above.
        result.content = vec![rmcp::model::ContentBlock::text(
            "super-secret-upstream-content",
        )];
        let analysis = analyze_completed_tool_error(&result, ToolSafetyHints::default())
            .expect("isError result");
        assert_eq!(analysis.kind, "tool_error");
        assert_eq!(analysis.original_kind.as_deref(), Some("server_error"));
        assert_eq!(analysis.cause.chars().count(), MAX_CAUSE_CHARS);
        assert_eq!(analysis.retry_after_ms, Some(250));
        assert_eq!(analysis.side_effects, ToolSideEffectRisk::Possible);
        let public = serde_json::to_string(&analysis).unwrap();
        assert!(!public.contains("super-secret-upstream-content"));
    }
}
