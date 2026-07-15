use crate::config::{
    GatewayUpstreamOauthConfig, GatewayUpstreamOauthMode, GatewayUpstreamOauthRegistration,
};

use super::*;

fn oauth_upstream() -> UpstreamConfig {
    UpstreamConfig {
        name: "oauth".to_owned(),
        url: Some("https://upstream.example/mcp".to_owned()),
        oauth: Some(GatewayUpstreamOauthConfig {
            mode: GatewayUpstreamOauthMode::AuthorizationCodePkce,
            registration: GatewayUpstreamOauthRegistration::Preregistered {
                client_id: "client".to_owned(),
                client_secret_env: Some("UPSTREAM_CLIENT_SECRET".to_owned()),
            },
            scopes: Some(vec!["read".to_owned()]),
            prefer_client_metadata_document: Some(true),
        }),
        ..UpstreamConfig::default()
    }
}

#[test]
fn adapts_gateway_config_into_soma_auth_leaf_config() {
    let auth_config = to_soma_auth_upstream_config(&oauth_upstream()).unwrap();

    assert_eq!(auth_config.name, "oauth");
    assert_eq!(
        auth_config.url.as_deref(),
        Some("https://upstream.example/mcp")
    );
    assert!(auth_config.oauth.is_some());
}

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
