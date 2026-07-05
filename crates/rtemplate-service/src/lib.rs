pub mod actions;
pub mod app;
pub mod example;

use anyhow::Result;
use rtemplate_contracts::{
    actions::action_validation_error,
    errors::{ServiceError, ToolError},
};
use serde_json::Value;

pub use actions::{
    action_registry, action_specs, execute_native_action, validate_params, ActionRegistry,
};
pub use app::{ElicitedNameOutcome, ExampleService, ScaffoldIntent, ScaffoldIntentValidationError};
pub use example::ExampleClient;

/// Unified dispatch seam shared by every surface (MCP, REST, CLI).
///
/// Wraps [`execute_service_action`] with consistent timing, structured logging,
/// and metrics so each surface gets identical observability for free. The shims
/// call this instead of `execute_service_action` directly; `execute_service_action`
/// remains public for callers that have already established their own span.
///
/// `surface` is a short, low-cardinality label such as `"mcp"`, `"rest"`, or
/// `"cli"`. Action *parameters* are intentionally never logged or labelled —
/// they can carry credentials, and per-value labels would explode metric
/// cardinality.
pub async fn dispatch_action(
    service: &ExampleService,
    action: &str,
    params: &Value,
    surface: &str,
) -> Result<Value> {
    let started = std::time::Instant::now();
    let result = execute_native_action(service, action, params).await;
    let elapsed_ms = started.elapsed().as_millis();
    let outcome = if result.is_ok() { "ok" } else { "error" };

    tracing::info!(
        surface,
        service = "example",
        action,
        outcome,
        elapsed_ms = elapsed_ms as u64,
        "action dispatched"
    );
    record_action_metric(surface, action, outcome, elapsed_ms as f64);

    result
}

#[cfg(feature = "observability")]
fn record_action_metric(surface: &str, action: &str, outcome: &str, elapsed_ms: f64) {
    metrics::counter!(
        "rtemplate_actions_total",
        "surface" => surface.to_owned(),
        "action" => action.to_owned(),
        "outcome" => outcome.to_owned(),
    )
    .increment(1);
    metrics::histogram!(
        "rtemplate_action_duration_ms",
        "surface" => surface.to_owned(),
        "action" => action.to_owned(),
    )
    .record(elapsed_ms);
}

#[cfg(not(feature = "observability"))]
fn record_action_metric(_surface: &str, _action: &str, _outcome: &str, _elapsed_ms: f64) {}

pub fn is_validation_error(error: &anyhow::Error) -> bool {
    classify_service_error(error).kind == rtemplate_contracts::errors::ServiceErrorKind::Validation
}

pub fn classify_service_error(error: &anyhow::Error) -> ServiceError {
    if let Some(error) = action_validation_error(error) {
        return ToolError::from_action_validation(error);
    }
    if let Some(error) = error.downcast_ref::<ScaffoldIntentValidationError>() {
        let mut tool_error =
            ToolError::validation(error.code(), error.to_string(), error.remediation());
        if let Some(field) = error.field() {
            tool_error = tool_error.with_field(field);
        }
        if let Some(expected_pattern) = error.expected_pattern() {
            tool_error = tool_error.with_expected_pattern(expected_pattern);
        }
        return tool_error;
    }
    ToolError::execution(error)
}
