use crate::gateway::protected_routes::ProtectedRouteScope;

use super::route_allowed;

#[test]
fn scoped_route_allows_only_named_upstreams() {
    let scope = ProtectedRouteScope {
        upstreams: vec!["media".to_owned()],
        services: Vec::new(),
        expose_code_mode: false,
    };

    assert!(route_allowed(Some(&scope), "media"));
    assert!(!route_allowed(Some(&scope), "admin"));
}

#[test]
fn missing_scope_keeps_root_gateway_unfiltered() {
    assert!(route_allowed(None, "admin"));
}
