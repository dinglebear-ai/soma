//! Soma-specific policy layered around provider-core dispatch.

use soma_domain::actions::scopes_satisfy;
use soma_domain::authz::{self, CallerContext};
use soma_provider_core::{HostCapabilities, ProviderKind, ProviderTool};

use crate::{capabilities::CapabilityBroker, provider_errors::ProviderError};

use super::{ProviderAuthMode, ProviderCall, ProviderOutput, ProviderPrincipal, ProviderSurface};

pub(super) fn enforce_pre_input(
    tool: &ProviderTool,
    call: &ProviderCall,
    provider_kind: ProviderKind,
) -> Result<(), ProviderError> {
    enforce_authz(provider_kind, call)?;
    enforce_scope(tool, call)?;
    enforce_admin(tool, call)?;
    enforce_destructive(tool, call)?;
    enforce_input_limit(tool, call)?;
    Ok(())
}

pub(super) fn enforce_capabilities(
    declared_capabilities: &HostCapabilities,
    call: &ProviderCall,
    capabilities: &CapabilityBroker,
) -> Result<(), ProviderError> {
    capabilities.authorize(&call.provider, &call.action, declared_capabilities)?;
    Ok(())
}

pub(crate) fn provider_tool_surface_enabled(tool: &ProviderTool, surface: ProviderSurface) -> bool {
    tool.exposed_on(match surface {
        ProviderSurface::Mcp => soma_provider_core::ProviderSurface::Mcp,
        ProviderSurface::Rest => soma_provider_core::ProviderSurface::Rest,
        ProviderSurface::Cli => soma_provider_core::ProviderSurface::Cli,
        ProviderSurface::Palette => soma_provider_core::ProviderSurface::Palette,
    })
}

/// Builds the domain [`CallerContext`] from what dispatch already knows.
/// Only the LoopbackDev and TrustedGateway auth modes — the two modes whose
/// scope checks were already bypassed before this layer existed — yield a
/// trusted-local caller; a Mounted (remote token) caller can never claim
/// local trust regardless of the scopes it presents.
fn caller_context(call: &ProviderCall) -> CallerContext {
    match call.auth_mode {
        ProviderAuthMode::LoopbackDev | ProviderAuthMode::TrustedGateway => {
            CallerContext::trusted_local_caller(call.principal.subject.clone())
        }
        ProviderAuthMode::Mounted => {
            if call.principal.scopes.is_empty() {
                CallerContext::anonymous()
            } else {
                CallerContext::remote_scoped(
                    call.principal.subject.clone(),
                    call.principal.scopes.clone(),
                )
            }
        }
    }
}

/// Safety-class affinity check for dynamic provider dispatch. Runs before
/// the per-tool declared-scope check: the class affinity is a floor derived
/// from what the provider's handler *kind* can do, independent of what the
/// tool manifest claims.
fn enforce_authz(provider_kind: ProviderKind, call: &ProviderCall) -> Result<(), ProviderError> {
    let decision = authz::authorize_provider_kind(&caller_context(call), provider_kind.as_str());
    for warning in &decision.warnings {
        tracing::debug!(
            provider = %call.provider,
            action = %call.action,
            reason = decision.reason,
            %warning,
            "provider authz warning"
        );
    }
    if decision.allowed {
        return Ok(());
    }
    let (code, remediation) = match decision.reason {
        authz::reasons::DENIED_SCOPE_MISSING => (
            "insufficient_scope",
            "Authenticate with a token whose scopes satisfy this provider kind's safety-class affinity.",
        ),
        authz::reasons::DENIED_AFFINITY_REQUIRES_LOCAL_TRUST => (
            "local_trust_required",
            "Invoke this action from the CLI/loopback surface or behind an explicitly trusted gateway; remote tokens cannot execute local-runtime providers.",
        ),
        _ => (
            "unclassified_provider_kind",
            "Add a safety classification for this provider kind before exposing it to dispatch.",
        ),
    };
    Err(ProviderError::new(
        code,
        &call.provider,
        Some(call.action.clone()),
        format!(
            "action `{}` on provider `{}` (kind `{}`) denied by authorization policy: {}",
            call.action,
            call.provider,
            provider_kind.as_str(),
            decision.reason
        ),
        remediation,
    ))
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
