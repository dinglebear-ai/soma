use soma_gateway::gateway::protected_routes::ProtectedRouteScope;

use super::protected_scope_allows_service;

#[test]
fn protected_scope_service_filter_is_closed_when_scope_is_present() {
    let scope = ProtectedRouteScope {
        upstreams: vec!["media".to_owned()],
        services: vec!["gateway".to_owned()],
        expose_code_mode: false,
    };

    assert!(protected_scope_allows_service(None, "soma"));
    assert!(protected_scope_allows_service(Some(&scope), "gateway"));
    assert!(!protected_scope_allows_service(Some(&scope), "soma"));
}
