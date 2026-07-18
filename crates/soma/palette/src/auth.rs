//! Product auth/session behavior for Palette requests.
//!
//! When the `auth` feature is enabled, this re-exports `soma_auth::AuthContext`
//! (populated by `soma-auth`'s middleware, mounted by the composing app) and
//! reads it the same way `soma-api` does. Without the feature, a minimal
//! local stand-in keeps this crate buildable standalone (matching
//! `soma-api`'s own pattern).

#[cfg(feature = "auth")]
pub use soma_auth::AuthContext;

#[cfg(not(feature = "auth"))]
#[derive(Clone)]
pub struct AuthContext {
    pub sub: String,
    pub scopes: Vec<String>,
}

use crate::state::PaletteState;

/// Build the [`soma_application::ExecutionContext`] for a Palette request,
/// pulling subject/scopes from `auth` when present (unauthenticated requests
/// under a loopback-dev or trusted-gateway policy pass `None`).
pub fn palette_execution_context(
    state: &PaletteState,
    auth: Option<&AuthContext>,
) -> soma_application::ExecutionContext {
    let scopes = auth.map(|auth| auth.scopes.as_slice()).unwrap_or_default();
    state.execution_context(auth.map(|auth| auth.sub.as_str()), scopes)
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
