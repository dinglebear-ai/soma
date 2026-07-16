use soma_application::ExecutionContext;
use soma_contracts::{actions::READ_SCOPE, scopes::ADMIN_SCOPE};
use soma_domain::{AuthorizationMode, Principal, RequestId, ScopeSet, Surface};

use super::gateway_access;

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
