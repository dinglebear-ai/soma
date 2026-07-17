use super::{has_admin_scope, ADMIN_SCOPE};

#[test]
fn detects_gateway_admin_scope() {
    let scopes = vec!["soma:read".to_owned(), ADMIN_SCOPE.to_owned()];

    assert!(has_admin_scope(&scopes));
}

#[test]
fn read_scope_is_not_gateway_admin() {
    let scopes = vec!["soma:read".to_owned()];

    assert!(!has_admin_scope(&scopes));
}
