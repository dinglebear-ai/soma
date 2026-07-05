pub mod app;
pub mod capabilities;
pub mod example;
pub mod provider_errors;
pub mod provider_registry;
pub mod providers;

use anyhow::{anyhow, Result};
use rtemplate_contracts::{
    actions::{action_validation_error, rest_help, ExampleAction},
    errors::{ServiceError, ToolError},
};
use serde_json::Value;

pub use app::{ElicitedNameOutcome, ExampleService, ScaffoldIntent, ScaffoldIntentValidationError};
pub use example::ExampleClient;
pub use provider_errors::ProviderError;
pub use provider_registry::{
    ProviderAuthMode, ProviderCall, ProviderOutput, ProviderPrincipal, ProviderRegistry,
    ProviderRequestLimits, ProviderSurface, RegistrySnapshot,
};
pub use providers::static_rust::StaticRustProvider;

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
    action: &ExampleAction,
    surface: &str,
) -> Result<Value> {
    let action_name = action.name();
    let started = std::time::Instant::now();
    let result = execute_service_action(service, action).await;
    let elapsed_ms = started.elapsed().as_millis();
    let outcome = if result.is_ok() { "ok" } else { "error" };

    tracing::info!(
        surface,
        service = "example",
        action = action_name,
        outcome,
        elapsed_ms = elapsed_ms as u64,
        "action dispatched"
    );
    record_action_metric(surface, action_name, outcome, elapsed_ms as f64);

    result
}

pub fn static_provider_registry(service: ExampleService) -> Result<ProviderRegistry> {
    ProviderRegistry::new(vec![std::sync::Arc::new(StaticRustProvider::new(service))])
        .map_err(|error| anyhow!(error.to_string()))
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

pub async fn execute_service_action(
    service: &ExampleService,
    action: &ExampleAction,
) -> Result<Value> {
    match action {
        ExampleAction::Greet { name } => service.greet(name.as_deref()).await,
        ExampleAction::Echo { message } => service.echo(message).await,
        ExampleAction::Status => service.status().await,
        ExampleAction::Help => Ok(rest_help()),
        ExampleAction::ElicitName => Err(anyhow!(
            "action=elicit_name is only available over MCP because it requires a peer"
        )),
        ExampleAction::ScaffoldIntent => Err(anyhow!(
            "action=scaffold_intent is only available over MCP because it requires elicitation"
        )),
    }
}

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
