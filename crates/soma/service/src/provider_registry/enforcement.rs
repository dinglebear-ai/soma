//! Soma-specific policy layered around provider-core dispatch.

use soma_contracts::{
    actions::scopes_satisfy,
    providers::{HostCapabilities, ProviderTool},
};

use crate::{capabilities::CapabilityBroker, provider_errors::ProviderError};

use super::{ProviderAuthMode, ProviderCall, ProviderOutput, ProviderPrincipal, ProviderSurface};

pub(super) fn enforce_call(
    tool: &ProviderTool,
    declared_capabilities: &HostCapabilities,
    call: &ProviderCall,
    capabilities: &CapabilityBroker,
) -> Result<(), ProviderError> {
    enforce_scope(tool, call)?;
    enforce_admin(tool, call)?;
    enforce_destructive(tool, call)?;
    enforce_input_limit(tool, call)?;
    capabilities.authorize(&call.provider, &call.action, declared_capabilities)?;
    Ok(())
}

pub(crate) fn provider_tool_surface_enabled(tool: &ProviderTool, surface: ProviderSurface) -> bool {
    tool.exposed_on(surface)
}

fn enforce_scope(tool: &ProviderTool, call: &ProviderCall) -> Result<(), ProviderError> {
    if !matches!(call.auth_mode, ProviderAuthMode::Mounted) {
        return Ok(());
    }
    let Some(scope) = tool.scope.as_deref() else {
        return Ok(());
    };
    if scopes_satisfy(&call.principal.scopes, scope) {
        return Ok(());
    }
    Err(ProviderError::new(
        "insufficient_scope",
        &call.provider,
        Some(call.action.clone()),
        format!("action `{}` requires scope `{scope}`", call.action),
        "Authenticate with a token that includes the required scope.",
    ))
}

fn enforce_admin(tool: &ProviderTool, call: &ProviderCall) -> Result<(), ProviderError> {
    if !tool.requires_admin || provider_principal_is_admin(&call.principal) {
        return Ok(());
    }
    Err(ProviderError::new(
        "admin_required",
        &call.provider,
        Some(call.action.clone()),
        format!("action `{}` requires an admin principal", call.action),
        "Authenticate with an admin-scoped token and retry.",
    ))
}

fn provider_principal_is_admin(principal: &ProviderPrincipal) -> bool {
    principal
        .scopes
        .iter()
        .any(|scope| scope == "admin" || scope == "soma:admin")
}

fn enforce_destructive(tool: &ProviderTool, call: &ProviderCall) -> Result<(), ProviderError> {
    if !tool.destructive || call.destructive_confirmed {
        return Ok(());
    }
    Err(ProviderError::validation(
        &call.provider,
        &call.action,
        "confirmation_required",
        format!(
            "action `{}` is destructive and requires confirmation",
            call.action
        ),
    ))
}

fn enforce_input_limit(tool: &ProviderTool, call: &ProviderCall) -> Result<(), ProviderError> {
    if tool
        .limits
        .as_ref()
        .and_then(|limits| limits.max_input_bytes)
        .is_some()
    {
        return Ok(());
    }
    let max = call.limits.max_input_bytes;
    let len = serde_json::to_vec(&call.params)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX);
    if len <= max {
        return Ok(());
    }
    Err(ProviderError::validation(
        &call.provider,
        &call.action,
        "input_too_large",
        format!("provider input exceeded {max} bytes"),
    ))
}

pub(super) fn enforce_response_limit(
    tool: &ProviderTool,
    call: &ProviderCall,
    output: &ProviderOutput,
) -> Result<(), ProviderError> {
    if tool
        .limits
        .as_ref()
        .and_then(|limits| limits.max_response_bytes)
        .is_some()
    {
        return Ok(());
    }
    let max = call.limits.max_response_bytes;
    let len = serde_json::to_vec(&output.value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX);
    if len <= max {
        return Ok(());
    }
    Err(ProviderError::new(
        "response_too_large",
        &call.provider,
        Some(call.action.clone()),
        format!("provider response exceeded {max} bytes"),
        "Reduce the response size or add paging before exposing this provider action.",
    ))
}

#[cfg(test)]
#[path = "enforcement_tests.rs"]
mod tests;
