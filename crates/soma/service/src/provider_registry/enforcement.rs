//! Tool-call enforcement: surface exposure, scope, admin, destructive
//! confirmation, and input/output schema and size checks run by
//! `ProviderRegistry::dispatch` before and after a provider's `call()`. Split
//! out of `provider_registry.rs` to stay under the module size hard limit —
//! no behavior change from the original inline functions.

use jsonschema::Validator;
use serde_json::Value;
use soma_contracts::{actions::scopes_satisfy, providers::ProviderTool};

use crate::{capabilities::CapabilityBroker, provider_errors::ProviderError};

use super::{
    ProviderAuthMode, ProviderCall, ProviderOutput, ProviderPrincipal, ProviderSurface, ToolEntry,
};

pub(super) fn enforce_call(
    entry: &ToolEntry,
    call: &ProviderCall,
    capabilities: &CapabilityBroker,
) -> Result<(), ProviderError> {
    enforce_surface(entry, call)?;
    enforce_scope(entry, call)?;
    enforce_admin(entry, call)?;
    enforce_destructive(entry, call)?;
    enforce_input_limit(entry, call)?;
    enforce_schema(entry, call)?;
    capabilities.authorize(&entry.provider, &entry.action, &entry.capabilities)?;
    Ok(())
}

fn enforce_surface(entry: &ToolEntry, call: &ProviderCall) -> Result<(), ProviderError> {
    let allowed = provider_tool_surface_enabled(&entry.tool, call.surface);
    if allowed {
        return Ok(());
    }
    Err(ProviderError::validation(
        &entry.provider,
        &entry.action,
        "surface_not_exposed",
        format!(
            "action `{}` is not exposed on {:?}",
            entry.action, call.surface
        ),
    ))
}

pub(crate) fn provider_tool_surface_enabled(tool: &ProviderTool, surface: ProviderSurface) -> bool {
    match surface {
        ProviderSurface::Mcp => tool.mcp.as_ref().map(|mcp| mcp.enabled).unwrap_or(true),
        ProviderSurface::Rest => tool.rest.as_ref().map(|rest| rest.enabled).unwrap_or(true),
        ProviderSurface::Cli => tool.cli.as_ref().map(|cli| cli.enabled).unwrap_or(false),
        ProviderSurface::Palette => tool
            .palette
            .as_ref()
            .map(|palette| palette.enabled)
            .unwrap_or(true),
    }
}

fn enforce_scope(entry: &ToolEntry, call: &ProviderCall) -> Result<(), ProviderError> {
    if !matches!(call.auth_mode, ProviderAuthMode::Mounted) {
        return Ok(());
    }
    let Some(scope) = entry.tool.scope.as_deref() else {
        return Ok(());
    };
    if scopes_satisfy(&call.principal.scopes, scope) {
        return Ok(());
    }
    Err(ProviderError::new(
        "insufficient_scope",
        &entry.provider,
        Some(entry.action.clone()),
        format!("action `{}` requires scope `{scope}`", entry.action),
        "Authenticate with a token that includes the required scope.",
    ))
}

fn enforce_admin(entry: &ToolEntry, call: &ProviderCall) -> Result<(), ProviderError> {
    if !entry.tool.requires_admin || provider_principal_is_admin(&call.principal) {
        return Ok(());
    }
    Err(ProviderError::new(
        "admin_required",
        &entry.provider,
        Some(entry.action.clone()),
        format!("action `{}` requires an admin principal", entry.action),
        "Authenticate with an admin-scoped token and retry.",
    ))
}

fn provider_principal_is_admin(principal: &ProviderPrincipal) -> bool {
    principal
        .scopes
        .iter()
        .any(|scope| scope == "admin" || scope == "soma:admin")
}

fn enforce_destructive(entry: &ToolEntry, call: &ProviderCall) -> Result<(), ProviderError> {
    if !entry.tool.destructive || call.destructive_confirmed {
        return Ok(());
    }
    Err(ProviderError::validation(
        &entry.provider,
        &entry.action,
        "confirmation_required",
        format!(
            "action `{}` is destructive and requires confirmation",
            entry.action
        ),
    ))
}

fn enforce_input_limit(entry: &ToolEntry, call: &ProviderCall) -> Result<(), ProviderError> {
    let max = entry
        .tool
        .limits
        .as_ref()
        .and_then(|limits| limits.max_input_bytes)
        .unwrap_or(call.limits.max_input_bytes);
    let len = serde_json::to_vec(&call.params)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX);
    if len <= max {
        return Ok(());
    }
    Err(ProviderError::validation(
        &entry.provider,
        &entry.action,
        "input_too_large",
        format!("provider input exceeded {max} bytes"),
    ))
}

fn schema_error_details(validator: &Validator, value: &Value) -> Option<String> {
    let errors = validator
        .iter_errors(value)
        .map(|error| format!("{}: {}", error.instance_path(), error))
        .collect::<Vec<_>>();
    (!errors.is_empty()).then(|| errors.join("; "))
}

fn enforce_schema(entry: &ToolEntry, call: &ProviderCall) -> Result<(), ProviderError> {
    if let Some(details) = schema_error_details(&entry.input_validator, &call.params) {
        return Err(ProviderError::validation(
            &entry.provider,
            &entry.action,
            "input_schema_failed",
            details,
        ));
    }
    Ok(())
}

pub(super) fn enforce_response_limit(
    entry: &ToolEntry,
    call: &ProviderCall,
    output: &ProviderOutput,
) -> Result<(), ProviderError> {
    let max = entry
        .tool
        .limits
        .as_ref()
        .and_then(|limits| limits.max_response_bytes)
        .unwrap_or(call.limits.max_response_bytes);
    let len = serde_json::to_vec(&output.value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX);
    if len <= max {
        return Ok(());
    }
    Err(ProviderError::new(
        "response_too_large",
        &entry.provider,
        Some(entry.action.clone()),
        format!("provider response exceeded {max} bytes"),
        "Reduce the response size or add paging before exposing this provider action.",
    ))
}

pub(super) fn enforce_output_schema(
    entry: &ToolEntry,
    output: &ProviderOutput,
) -> Result<(), ProviderError> {
    let Some(output_validator) = &entry.output_validator else {
        return Ok(());
    };
    if let Some(details) = schema_error_details(output_validator, &output.value) {
        return Err(ProviderError::new(
            "output_schema_failed",
            &entry.provider,
            Some(entry.action.clone()),
            details,
            "Fix the provider output or its declared output_schema, then retry.",
        )
        .with_phase("output_validation"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "enforcement_tests.rs"]
mod tests;
