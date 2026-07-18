use soma_domain::{AuthorizationMode, Surface};
use soma_test_support::default_application;

use super::{palette_execution_context, AuthContext};
use crate::state::PaletteState;

#[cfg(feature = "auth")]
fn make_auth_context(sub: &str, scopes: Vec<String>) -> AuthContext {
    AuthContext {
        sub: sub.to_string(),
        actor_key: None,
        scopes,
        issuer: "test".to_string(),
        via_session: false,
        csrf_token: None,
        email: None,
    }
}

#[cfg(not(feature = "auth"))]
fn make_auth_context(sub: &str, scopes: Vec<String>) -> AuthContext {
    AuthContext {
        sub: sub.to_string(),
        scopes,
    }
}

fn state() -> PaletteState {
    PaletteState::new(default_application(), AuthorizationMode::Mounted)
}

#[test]
fn no_auth_context_produces_no_principal_with_palette_surface() {
    let state = state();
    let context = palette_execution_context(&state, None);

    assert!(context.principal.is_none());
    assert_eq!(context.surface, Surface::Palette);
    assert_eq!(context.authorization_mode, AuthorizationMode::Mounted);
}

#[test]
fn auth_context_scopes_and_subject_carry_into_execution_context() {
    let state = state();
    let auth = make_auth_context(
        "user-42",
        vec!["soma:read".to_string(), "soma:write".to_string()],
    );

    let context = palette_execution_context(&state, Some(&auth));

    let principal = context.principal.expect("principal should be set");
    assert_eq!(principal.subject, "user-42");
    assert!(principal.scopes.contains("soma:read"));
    assert!(principal.scopes.contains("soma:write"));
    assert_eq!(context.surface, Surface::Palette);
}
