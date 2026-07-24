//! Provider-dispatch authorization: safety-class → scope affinity, caller
//! trust discipline, and structured security decisions.
//!
//! Modeled on axon's `axon-authz` execution-affinity layer, adapted to
//! Soma's provider kinds and its stricter write-satisfies-read scope rule
//! (`crate::scopes::scopes_satisfy`). This layer governs **dynamic provider
//! execution only** — built-in actions keep their `ACTION_SPECS` scope
//! checks in the MCP server layer.
//!
//! Three invariants this module enforces by construction:
//!
//! 1. **Affinity**: every classified dispatch target has a minimum scope
//!    derived from what its handler *can do* (execute code, egress to the
//!    network, …), independent of the scope the tool manifest declares.
//! 2. **Trusted-local discipline**: exactly one constructor
//!    ([`CallerContext::trusted_local_caller`]) can set `trusted_local`;
//!    the remote/scoped constructor hard-codes it to `false`, so a network
//!    caller can never claim local trust.
//! 3. **Deny-by-default**: an unclassified target denies with a stable
//!    machine-readable reason instead of falling through to "allow".

use crate::scopes::{scopes_satisfy, WRITE_SCOPE};

/// Stable machine-readable decision reasons. These are API: never rename an
/// existing constant's value, only add new ones.
pub mod reasons {
    pub const AUTHORIZED_TRUSTED_LOCAL: &str = "authorized.trusted_local";
    pub const AUTHORIZED_SCOPE_SATISFIED: &str = "authorized.scope_satisfied";
    pub const AUTHORIZED_NO_AFFINITY_REQUIRED: &str = "authorized.no_affinity_required";
    pub const DENIED_SCOPE_MISSING: &str = "denied.scope_missing";
    pub const DENIED_AFFINITY_REQUIRES_LOCAL_TRUST: &str = "denied.affinity_requires_local_trust";
    pub const DENIED_UNCLASSIFIED_TARGET: &str = "denied.unclassified_target";
}

/// What a dynamic provider's handler is capable of doing when invoked,
/// classified from the provider manifest `kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafetyClass {
    /// Trusted Rust compiled into this binary (`static-rust`).
    InProcessTrusted,
    /// Guest code executed inside a sandboxed runtime (`wasm`).
    SandboxedExecution,
    /// Handlers executed by a local script runtime with host access
    /// (`ai-sdk` TypeScript, `python`, `langchain`, `llamaindex`).
    LocalRuntimeExecution,
    /// Handlers whose execution is a network call to an upstream service
    /// (`mcp`, `openapi`).
    NetworkEgress,
}

impl SafetyClass {
    /// Classifies a provider manifest kind string. Unknown kinds return
    /// `None`, which [`authorize`] denies with
    /// [`reasons::DENIED_UNCLASSIFIED_TARGET`].
    pub fn classify_provider_kind(kind: &str) -> Option<Self> {
        match kind {
            "static-rust" => Some(Self::InProcessTrusted),
            "wasm" => Some(Self::SandboxedExecution),
            "ai-sdk" | "python" | "langchain" | "llamaindex" => Some(Self::LocalRuntimeExecution),
            "mcp" | "openapi" => Some(Self::NetworkEgress),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::InProcessTrusted => "in-process-trusted",
            Self::SandboxedExecution => "sandboxed-execution",
            Self::LocalRuntimeExecution => "local-runtime-execution",
            Self::NetworkEgress => "network-egress",
        }
    }

    /// Whether inline execution of this class additionally requires a
    /// trusted-local caller even when the affinity scope is held.
    pub fn requires_local_trust_when_inline(self) -> bool {
        matches!(self, Self::LocalRuntimeExecution)
    }
}

/// Minimum scope a caller must hold to execute a target of this class,
/// regardless of the scope declared on the individual tool. The tool's own
/// declared scope is still enforced separately on top of this floor.
///
/// `InProcessTrusted` has no floor (`None`): static Rust compiled into this
/// binary is trusted code whose manifest scopes govern entirely — this is
/// what keeps public tools (e.g. `help`, which declares no scope) reachable
/// by unauthenticated callers.
pub fn required_scope_for_safety_class(class: SafetyClass) -> Option<&'static str> {
    match class {
        SafetyClass::InProcessTrusted => None,
        SafetyClass::SandboxedExecution
        | SafetyClass::LocalRuntimeExecution
        | SafetyClass::NetworkEgress => Some(WRITE_SCOPE),
    }
}

/// Where a handler executes relative to the Soma host process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Executes on the host (in-process, or a local runtime the host
    /// spawns with host-level access).
    Inline,
    /// Executes inside an isolation boundary (e.g. a WASM sandbox).
    Sandboxed,
}

impl ExecutionMode {
    /// Execution mode implied by a provider manifest kind: only `wasm`
    /// handlers run behind an isolation boundary today.
    pub fn for_provider_kind(kind: &str) -> Self {
        if kind == "wasm" {
            Self::Sandboxed
        } else {
            Self::Inline
        }
    }
}

/// Who is asking for a dispatch. Fields are private on purpose: the only
/// way to obtain `trusted_local = true` is [`Self::trusted_local_caller`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerContext {
    subject: String,
    scopes: Vec<String>,
    trusted_local: bool,
}

impl CallerContext {
    /// The one constructor that grants local trust. Reserved for callers the
    /// process itself vouches for: the CLI/loopback path and an explicitly
    /// configured authz-enforcing trusted gateway.
    pub fn trusted_local_caller(subject: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            scopes: Vec::new(),
            trusted_local: true,
        }
    }

    /// A remote caller carrying token scopes. `trusted_local` is hard-coded
    /// `false`: no scope set a network caller presents can confer local trust.
    pub fn remote_scoped(subject: impl Into<String>, scopes: Vec<String>) -> Self {
        Self {
            subject: subject.into(),
            scopes,
            trusted_local: false,
        }
    }

    /// A caller that grants nothing: no scopes, no local trust.
    pub fn anonymous() -> Self {
        Self {
            subject: "anonymous".to_owned(),
            scopes: Vec::new(),
            trusted_local: false,
        }
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }

    pub fn is_trusted_local(&self) -> bool {
        self.trusted_local
    }
}

/// A decision object, not a bool: `allowed` plus a stable machine-readable
/// `reason` from [`reasons`], plus human-oriented advisory `warnings`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityDecision {
    pub allowed: bool,
    pub reason: &'static str,
    pub warnings: Vec<String>,
}

impl SecurityDecision {
    pub fn allow(reason: &'static str) -> Self {
        Self {
            allowed: true,
            reason,
            warnings: Vec::new(),
        }
    }

    pub fn deny(reason: &'static str) -> Self {
        Self {
            allowed: false,
            reason,
            warnings: Vec::new(),
        }
    }

    pub fn is_denied(&self) -> bool {
        !self.allowed
    }
}

/// Authorizes one dispatch: affinity scope + the trusted-local inline rule,
/// deny-by-default for unclassified targets.
///
/// Trusted-local callers bypass affinity entirely — this keeps
/// LoopbackDev/TrustedGateway behavior identical to the pre-authz world,
/// where scope checks were skipped outside `Mounted` auth.
pub fn authorize(
    caller: &CallerContext,
    safety_class: Option<SafetyClass>,
    execution_mode: ExecutionMode,
) -> SecurityDecision {
    let Some(class) = safety_class else {
        return SecurityDecision::deny(reasons::DENIED_UNCLASSIFIED_TARGET);
    };
    if caller.is_trusted_local() {
        let mut decision = SecurityDecision::allow(reasons::AUTHORIZED_TRUSTED_LOCAL);
        if required_scope_for_safety_class(class).is_some() {
            decision.warnings.push(format!(
                "trusted-local caller `{}` bypassed `{WRITE_SCOPE}` scope affinity for safety class `{}`",
                caller.subject(),
                class.as_str(),
            ));
        }
        return decision;
    }
    let Some(required) = required_scope_for_safety_class(class) else {
        return SecurityDecision::allow(reasons::AUTHORIZED_NO_AFFINITY_REQUIRED);
    };
    if !scopes_satisfy(caller.scopes(), required) {
        return SecurityDecision::deny(reasons::DENIED_SCOPE_MISSING);
    }
    if execution_mode == ExecutionMode::Inline && class.requires_local_trust_when_inline() {
        return SecurityDecision::deny(reasons::DENIED_AFFINITY_REQUIRES_LOCAL_TRUST);
    }
    SecurityDecision::allow(reasons::AUTHORIZED_SCOPE_SATISFIED)
}

/// Convenience for the provider-dispatch chokepoint: classify a manifest
/// kind, derive its execution mode, and authorize in one call.
pub fn authorize_provider_kind(caller: &CallerContext, provider_kind: &str) -> SecurityDecision {
    authorize(
        caller,
        SafetyClass::classify_provider_kind(provider_kind),
        ExecutionMode::for_provider_kind(provider_kind),
    )
}

#[cfg(test)]
#[path = "authz_tests.rs"]
mod tests;
