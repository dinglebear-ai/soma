use soma_contracts::actions::{READ_SCOPE, WRITE_SCOPE};
use soma_contracts::scopes::ADMIN_SCOPE;
use soma_runtime::server::AuthPolicy;

use super::gateway_access_from_scopes;

#[test]
fn mounted_read_scope_gets_gateway_read_only_access() {
    let access = gateway_access_from_scopes(&mounted_policy(), &[READ_SCOPE.to_owned()]);

    assert!(access.read);
    assert!(!access.admin);
}

#[test]
fn mounted_admin_scope_gets_gateway_admin_access() {
    let access = gateway_access_from_scopes(&mounted_policy(), &[ADMIN_SCOPE.to_owned()]);

    assert!(access.read);
    assert!(access.admin);
}

#[test]
fn mounted_write_scope_does_not_imply_gateway_admin() {
    let access = gateway_access_from_scopes(&mounted_policy(), &[WRITE_SCOPE.to_owned()]);

    assert!(access.read);
    assert!(!access.admin);
}

#[test]
fn loopback_bypasses_gateway_scope_checks() {
    let access = gateway_access_from_scopes(&AuthPolicy::LoopbackDev, &[]);

    assert!(access.read);
    assert!(access.admin);
}

#[test]
fn trusted_gateway_bypasses_gateway_scope_checks() {
    let access = gateway_access_from_scopes(&AuthPolicy::TrustedGatewayUnscoped, &[]);

    assert!(access.read);
    assert!(access.admin);
}

#[cfg(feature = "auth")]
fn mounted_policy() -> AuthPolicy {
    AuthPolicy::Mounted { auth_state: None }
}

#[cfg(not(feature = "auth"))]
fn mounted_policy() -> AuthPolicy {
    AuthPolicy::Mounted {}
}
