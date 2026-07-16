use super::*;

#[test]
fn identity_matrix_accepts_caller_subject_only_for_admin_oauth() {
    let rows = identity_matrix();
    assert!(
        rows.iter()
            .find(|row| row.surface == GatewayOAuthSurface::AdminOAuthOperation)
            .unwrap()
            .caller_supplied_subject_accepted
    );
    assert!(rows
        .iter()
        .filter(|row| row.surface != GatewayOAuthSurface::AdminOAuthOperation)
        .all(|row| !row.caller_supplied_subject_accepted));

    assert_eq!(
        resolve_subject(
            GatewayOAuthSurface::ProtectedPublicRoute,
            "shared",
            Some("attacker")
        ),
        Err(GatewayOAuthError::CallerSuppliedSubjectDenied)
    );
}

#[test]
fn protected_routes_strip_public_authorization_before_upstream_auth() {
    let headers = strip_public_authorization_header([
        ("Authorization", "Bearer public"),
        ("accept", "application/json"),
    ]);

    assert_eq!(headers, vec![("accept", "application/json")]);
}
