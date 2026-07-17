//! Smoke test for the deprecated facade: confirms `soma_contracts::actions`
//! still resolves to the real `soma_domain::actions` implementation. Full
//! behavioral coverage lives in `soma-domain`.

use super::*;

#[test]
fn facade_reexports_the_real_action_catalog() {
    assert!(is_known_action("echo"));
    assert_eq!(required_scope_for_action("echo"), Some(READ_SCOPE));
    assert!(scopes_satisfy(&[WRITE_SCOPE.to_owned()], READ_SCOPE));
}
