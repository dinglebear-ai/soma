use soma_domain::actions::READ_SCOPE;

use super::{principal, protected_scope_allows_service};
use crate::state::McpRouteScope;

#[test]
fn protected_scope_service_filter_is_closed_when_scope_is_present() {
    let scope = McpRouteScope {
        upstreams: vec!["media".to_owned()],
        services: vec!["gateway".to_owned()],
        expose_code_mode: false,
    };

    assert!(protected_scope_allows_service(None, "soma"));
    assert!(protected_scope_allows_service(Some(&scope), "gateway"));
    assert!(!protected_scope_allows_service(Some(&scope), "soma"));
}

#[test]
fn context_free_auth_preserves_loopback_identity() {
    let principal = principal(None);

    assert_eq!(principal.subject, "loopback-dev");
    assert!(principal.scopes.contains(READ_SCOPE));
}

#[cfg(not(feature = "auth"))]
#[test]
fn auth_context_becomes_domain_principal() {
    let auth = super::AuthContext {
        sub: "caller-1".to_owned(),
        scopes: vec!["soma:write".to_owned(), "soma:read".to_owned()],
    };

    let principal = principal(Some(&auth));

    assert_eq!(principal.subject, "caller-1");
    assert_eq!(
        principal.scopes.to_vec(),
        vec!["soma:read".to_owned(), "soma:write".to_owned()]
    );
}
