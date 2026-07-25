use super::{
    authorize, authorize_provider_kind, reasons, required_scope_for_safety_class, CallerContext,
    ExecutionMode, SafetyClass,
};
use crate::scopes::{READ_SCOPE, WRITE_SCOPE};

const READ_ONLY: &[&str] = &[READ_SCOPE];

fn remote(scopes: &[&str]) -> CallerContext {
    CallerContext::remote_scoped(
        "remote-user",
        scopes.iter().map(|s| (*s).to_owned()).collect(),
    )
}

#[test]
fn only_the_trusted_local_constructor_grants_local_trust() {
    assert!(CallerContext::trusted_local_caller("cli").is_trusted_local());
    assert!(!remote(&[READ_SCOPE, WRITE_SCOPE, "soma:admin"]).is_trusted_local());
    assert!(!CallerContext::anonymous().is_trusted_local());
}

#[test]
fn anonymous_caller_grants_nothing() {
    let anonymous = CallerContext::anonymous();
    assert!(anonymous.scopes().is_empty());
    assert!(!anonymous.is_trusted_local());
    for kind in ["wasm", "ai-sdk", "mcp", "openapi"] {
        let decision = authorize_provider_kind(&anonymous, kind);
        assert!(decision.is_denied(), "kind `{kind}`");
        assert_eq!(
            decision.reason,
            reasons::DENIED_SCOPE_MISSING,
            "kind `{kind}`"
        );
    }
}

#[test]
fn provider_kinds_classify_to_expected_safety_classes() {
    let expected = [
        ("static-rust", SafetyClass::InProcessTrusted),
        ("wasm", SafetyClass::SandboxedExecution),
        ("ai-sdk", SafetyClass::LocalRuntimeExecution),
        ("python", SafetyClass::LocalRuntimeExecution),
        ("langchain", SafetyClass::LocalRuntimeExecution),
        ("llamaindex", SafetyClass::LocalRuntimeExecution),
        ("mcp", SafetyClass::NetworkEgress),
        ("openapi", SafetyClass::NetworkEgress),
    ];
    for (kind, class) in expected {
        assert_eq!(
            SafetyClass::classify_provider_kind(kind),
            Some(class),
            "kind `{kind}`"
        );
    }
    assert_eq!(SafetyClass::classify_provider_kind("cgi-bin"), None);
}

#[test]
fn affinity_maps_each_class_to_its_scope_floor() {
    assert_eq!(
        required_scope_for_safety_class(SafetyClass::InProcessTrusted),
        None,
        "trusted in-process code has no affinity floor; its manifest scopes govern"
    );
    assert_eq!(
        required_scope_for_safety_class(SafetyClass::SandboxedExecution),
        Some(WRITE_SCOPE)
    );
    assert_eq!(
        required_scope_for_safety_class(SafetyClass::LocalRuntimeExecution),
        Some(WRITE_SCOPE)
    );
    assert_eq!(
        required_scope_for_safety_class(SafetyClass::NetworkEgress),
        Some(WRITE_SCOPE)
    );
}

#[test]
fn only_wasm_kind_executes_sandboxed() {
    assert_eq!(
        ExecutionMode::for_provider_kind("wasm"),
        ExecutionMode::Sandboxed
    );
    for kind in ["static-rust", "ai-sdk", "python", "mcp", "openapi", "??"] {
        assert_eq!(
            ExecutionMode::for_provider_kind(kind),
            ExecutionMode::Inline,
            "kind `{kind}`"
        );
    }
}

#[test]
fn scoped_caller_with_affinity_scope_is_authorized() {
    let decision = authorize_provider_kind(&remote(&[WRITE_SCOPE]), "openapi");
    assert!(decision.allowed);
    assert_eq!(decision.reason, reasons::AUTHORIZED_SCOPE_SATISFIED);
    assert!(decision.warnings.is_empty());

    let decision = authorize_provider_kind(&remote(&[WRITE_SCOPE]), "mcp");
    assert!(decision.allowed);
    assert_eq!(decision.reason, reasons::AUTHORIZED_SCOPE_SATISFIED);
}

#[test]
fn in_process_trusted_targets_have_no_affinity_floor() {
    // Even an anonymous remote caller passes the affinity layer for
    // static-rust providers — the tool's own declared scope (enforced
    // separately) is the sole gate, which keeps public tools public.
    let decision = authorize_provider_kind(&CallerContext::anonymous(), "static-rust");
    assert!(decision.allowed);
    assert_eq!(decision.reason, reasons::AUTHORIZED_NO_AFFINITY_REQUIRED);

    let decision = authorize_provider_kind(&remote(READ_ONLY), "static-rust");
    assert!(decision.allowed);
    assert_eq!(decision.reason, reasons::AUTHORIZED_NO_AFFINITY_REQUIRED);
}

#[test]
fn read_scope_does_not_satisfy_write_affinity_classes() {
    for kind in ["wasm", "ai-sdk", "python", "mcp", "openapi"] {
        let decision = authorize_provider_kind(&remote(READ_ONLY), kind);
        assert!(decision.is_denied(), "kind `{kind}`");
        assert_eq!(
            decision.reason,
            reasons::DENIED_SCOPE_MISSING,
            "kind `{kind}`"
        );
    }
}

#[test]
fn inline_local_runtime_execution_requires_local_trust_even_with_scope() {
    for kind in ["ai-sdk", "python", "langchain", "llamaindex"] {
        let decision = authorize_provider_kind(&remote(&[WRITE_SCOPE]), kind);
        assert!(decision.is_denied(), "kind `{kind}`");
        assert_eq!(
            decision.reason,
            reasons::DENIED_AFFINITY_REQUIRES_LOCAL_TRUST,
            "kind `{kind}`"
        );
    }
}

#[test]
fn sandboxed_execution_of_local_runtime_class_does_not_require_local_trust() {
    let decision = authorize(
        &remote(&[WRITE_SCOPE]),
        Some(SafetyClass::LocalRuntimeExecution),
        ExecutionMode::Sandboxed,
    );
    assert!(decision.allowed);
    assert_eq!(decision.reason, reasons::AUTHORIZED_SCOPE_SATISFIED);
}

#[test]
fn trusted_local_caller_bypasses_affinity_and_local_trust_rules() {
    let trusted = CallerContext::trusted_local_caller("loopback-dev");
    for kind in [
        "static-rust",
        "wasm",
        "ai-sdk",
        "python",
        "langchain",
        "llamaindex",
        "mcp",
        "openapi",
    ] {
        let decision = authorize_provider_kind(&trusted, kind);
        assert!(decision.allowed, "kind `{kind}`");
        assert_eq!(
            decision.reason,
            reasons::AUTHORIZED_TRUSTED_LOCAL,
            "kind `{kind}`"
        );
    }
}

#[test]
fn trusted_local_bypass_of_write_affinity_carries_a_warning() {
    let trusted = CallerContext::trusted_local_caller("loopback-dev");
    let bypassed = authorize_provider_kind(&trusted, "python");
    assert_eq!(bypassed.warnings.len(), 1);
    assert!(bypassed.warnings[0].contains("local-runtime-execution"));

    let read_class = authorize_provider_kind(&trusted, "static-rust");
    assert!(read_class.warnings.is_empty());
}

#[test]
fn unclassified_targets_deny_by_default() {
    let decision = authorize_provider_kind(&remote(&[WRITE_SCOPE, "soma:admin"]), "shell-script");
    assert!(decision.is_denied());
    assert_eq!(decision.reason, reasons::DENIED_UNCLASSIFIED_TARGET);

    // Even a trusted-local caller cannot execute an unclassified target.
    let trusted = CallerContext::trusted_local_caller("cli");
    let decision = authorize_provider_kind(&trusted, "shell-script");
    assert!(decision.is_denied());
    assert_eq!(decision.reason, reasons::DENIED_UNCLASSIFIED_TARGET);
}
