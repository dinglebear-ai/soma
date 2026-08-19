use soma_application::ExecutionContext;
use soma_domain::{
    AuthorizationMode, Principal, RequestId, ScopeSet, Surface, actions::READ_SCOPE,
    scopes::ADMIN_SCOPE,
};

use super::{gateway_access, gateway_manager_port_error, gateway_subject};

fn mounted_context(scopes: &[&str]) -> ExecutionContext {
    let mut context =
        ExecutionContext::loopback(Surface::Rest, RequestId::new("rest-test").unwrap());
    context.authorization_mode = AuthorizationMode::Mounted;
    context.principal = Some(Principal::new(
        "caller",
        ScopeSet::new(scopes.iter().map(|scope| (*scope).to_owned())),
    ));
    context
}

#[test]
fn tool_execution_analysis_becomes_actionable_port_error_details() {
    let error = soma_gateway::gateway::manager::GatewayManagerError::Upstream(
        soma_gateway::upstream::UpstreamError::ToolExecution {
            upstream: "demo".to_owned(),
            tool: "read".to_owned(),
            analysis: Box::new(soma_gateway::upstream::ToolExecutionAnalysis {
                contract_version: 1,
                kind: "rate_limited".to_owned(),
                original_kind: Some("rate_limited".to_owned()),
                cause: "slow down token=super-secret".to_owned(),
                retry_after_ms: Some(250),
                safety: soma_gateway::upstream::ToolSafetyHints {
                    read_only_hint: Some(true),
                    ..Default::default()
                },
                recovery: soma_gateway::upstream::ToolRecoveryAdvice {
                    action: soma_gateway::upstream::ToolRecoveryAction::RetryLater,
                    same_arguments: soma_gateway::upstream::SameArgumentsRetry::Conditional,
                    guidance: "wait and retry".to_owned(),
                    retry_after_ms: Some(250),
                },
                side_effects: soma_gateway::upstream::ToolSideEffectRisk::NoneExpected,
            }),
        },
    );

    let port = gateway_manager_port_error("tools/call", error);
    assert_eq!(port.code, "tool_execution_failed");
    assert!(port.retryable);
    assert_eq!(port.remediation, "wait and retry");
    let details = port.details.expect("tool analysis details");
    assert_eq!(details["kind"], "rate_limited");
    assert_eq!(details["cause"], "[redacted provider diagnostic]");
    assert!(!details.to_string().contains("super-secret"));
    assert_eq!(details["retry_after_ms"], 250);
    assert_eq!(details["recovery"]["action"], "retry_later");
    assert_eq!(details["side_effects"], "none_expected");
}

#[test]
fn mounted_gateway_access_distinguishes_read_and_admin_scopes() {
    let read = gateway_access(&mounted_context(&[READ_SCOPE]));
    assert!(read.read);
    assert!(!read.admin);

    let admin = gateway_access(&mounted_context(&[ADMIN_SCOPE]));
    assert!(admin.read);
    assert!(admin.admin);
}

#[test]
fn mounted_gateway_subject_preserves_per_user_oauth_identity() {
    let context = mounted_context(&[READ_SCOPE]);

    assert_eq!(gateway_subject(&context), "caller");
}

#[test]
fn local_and_admin_principals_use_shared_gateway_credentials() {
    let mut local = mounted_context(&[READ_SCOPE]);
    local.principal = local
        .principal
        .take()
        .map(|principal| principal.with_issuer("local"));
    let admin = mounted_context(&[ADMIN_SCOPE]);

    assert_eq!(gateway_subject(&local), "gateway");
    assert_eq!(gateway_subject(&admin), "gateway");
}

#[test]
fn non_mounted_gateway_subject_uses_shared_credentials() {
    let context = ExecutionContext::loopback(Surface::Mcp, RequestId::new("mcp-test").unwrap());

    assert_eq!(gateway_subject(&context), "gateway");
}
