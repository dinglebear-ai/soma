//! Soma's product auth default mapping: the env prefix, session cookie name,
//! supported/default scopes, and resource path that turn `soma-auth`'s
//! generic, product-agnostic `AuthConfigBuilder` into Soma's own OAuth
//! configuration (plan section 3.20).
//!
//! Moved out of `apps/soma/src/runtime.rs` (formerly the private
//! `soma_mcp_auth_config_builder` helper) — this crate is its permanent home
//! per PR 11's acceptance criterion that `apps/soma` constructs adapters but
//! contains none of their implementation logic.

/// Soma's default `soma_auth::config::AuthConfigBuilder`: `SOMA_MCP` env
/// prefix, `soma_mcp_session` cookie, the three Soma scopes, `soma:read` as
/// default scope, `/mcp` as the OAuth resource path, and dynamic client
/// registration enabled.
pub fn soma_auth_config_builder() -> soma_auth::config::AuthConfigBuilder {
    soma_auth::config::AuthConfigBuilder::new()
        .env_prefix("SOMA_MCP")
        .session_cookie_name("soma_mcp_session")
        .scopes_supported(vec![
            soma_domain::actions::READ_SCOPE.into(),
            soma_domain::actions::WRITE_SCOPE.into(),
            soma_domain::scopes::ADMIN_SCOPE.into(),
        ])
        .default_scope("soma:read")
        .resource_path("/mcp")
        .enable_dynamic_registration(true)
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
