use super::rest_principal;

#[test]
fn missing_rest_auth_context_uses_anonymous_principal() {
    let principal = rest_principal(None);

    assert_eq!(principal.subject, "anonymous");
    assert!(principal.scopes.is_empty());
}
