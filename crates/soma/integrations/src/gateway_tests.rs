use soma_application::ExecutionContext;
use soma_domain::{
    actions::READ_SCOPE, scopes::ADMIN_SCOPE, AuthorizationMode, Principal, RequestId, ScopeSet,
    Surface,
};

use super::{gateway_access, gateway_subject};

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
