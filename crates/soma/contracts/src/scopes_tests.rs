//! Smoke test for the deprecated facade: confirms `soma_contracts::scopes`
//! still resolves to the real `soma_domain::scopes` implementation. Full
//! behavioral coverage lives in `soma-domain`.

use super::*;

#[test]
fn facade_reexports_the_real_admin_scope() {
    assert_eq!(ADMIN_SCOPE, "soma:admin");
    assert!(has_admin_scope(&[ADMIN_SCOPE.to_owned()]));
}
