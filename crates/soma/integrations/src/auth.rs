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

/// Build Soma's `soma_auth::config::AuthConfig` from the typed
/// `soma_config::AuthConfig` — the only supported path for the OAuth runtime.
///
/// `soma_config::Config::load` is the single reader of `SOMA_MCP_*` process
/// env; this function feeds [`soma_auth_config_builder`] a var list
/// synthesized from those typed fields via [`soma_auth_env_vars`], so
/// `soma-auth` itself never touches process env in Soma's OAuth path (the
/// pattern cortex uses with lab-auth).
pub fn soma_auth_config(
    auth: &soma_config::AuthConfig,
) -> Result<soma_auth::config::AuthConfig, soma_auth::error::AuthError> {
    soma_auth_config_builder().build_from_sources(soma_auth_env_vars(auth))
}

/// Synthesize the `SOMA_MCP_*` var list `soma_auth::AuthConfigBuilder`
/// expects from typed config fields.
///
/// Unset options (and empty strings/lists) are omitted entirely so the auth
/// crate applies its own defaults (`crates/shared/auth/src/config.rs`) —
/// soma-config deliberately duplicates none of those default values.
pub fn soma_auth_env_vars(auth: &soma_config::AuthConfig) -> Vec<(String, String)> {
    let mut vars: Vec<(String, String)> = Vec::new();
    let mut push = |suffix: &str, value: String| {
        if !value.trim().is_empty() {
            vars.push((format!("SOMA_MCP_{suffix}"), value));
        }
    };
    let push_opt = |push: &mut dyn FnMut(&str, String), suffix: &str, value: &Option<String>| {
        if let Some(value) = value {
            push(suffix, value.clone());
        }
    };
    let push_csv = |push: &mut dyn FnMut(&str, String), suffix: &str, values: &[String]| {
        if !values.is_empty() {
            push(suffix, values.join(","));
        }
    };

    push(
        "AUTH_MODE",
        match auth.mode {
            soma_config::AuthMode::Bearer => "bearer".to_owned(),
            soma_config::AuthMode::OAuth => "oauth".to_owned(),
        },
    );
    push_opt(&mut push, "PUBLIC_URL", &auth.public_url);
    push("AUTH_ADMIN_EMAIL", auth.admin_email.clone());
    push_opt(&mut push, "AUTH_BOOTSTRAP_SECRET", &auth.bootstrap_secret);
    push_opt(&mut push, "AUTH_SQLITE_PATH", &auth.sqlite_path);
    push_opt(&mut push, "AUTH_KEY_PATH", &auth.key_path);
    push_csv(
        &mut push,
        "AUTH_ALLOWED_REDIRECT_URIS",
        &auth.allowed_client_redirect_uris,
    );
    push_opt(&mut push, "GOOGLE_CLIENT_ID", &auth.google_client_id);
    push_opt(
        &mut push,
        "GOOGLE_CLIENT_SECRET",
        &auth.google_client_secret,
    );
    push_opt(
        &mut push,
        "GOOGLE_CALLBACK_PATH",
        &auth.google_callback_path,
    );
    push_csv(&mut push, "GOOGLE_SCOPES", &auth.google_scopes);
    push_opt(&mut push, "AUTHELIA_ISSUER_URL", &auth.authelia_issuer_url);
    push_opt(&mut push, "AUTHELIA_CLIENT_ID", &auth.authelia_client_id);
    push_opt(
        &mut push,
        "AUTHELIA_CLIENT_SECRET",
        &auth.authelia_client_secret,
    );
    push_opt(
        &mut push,
        "AUTHELIA_CALLBACK_PATH",
        &auth.authelia_callback_path,
    );
    push_csv(&mut push, "AUTHELIA_SCOPES", &auth.authelia_scopes);
    push_opt(&mut push, "GITHUB_CLIENT_ID", &auth.github_client_id);
    push_opt(
        &mut push,
        "GITHUB_CLIENT_SECRET",
        &auth.github_client_secret,
    );
    push_opt(
        &mut push,
        "GITHUB_CALLBACK_PATH",
        &auth.github_callback_path,
    );
    push_csv(&mut push, "GITHUB_SCOPES", &auth.github_scopes);
    push_opt(&mut push, "AUTH_DEFAULT_PROVIDER", &auth.default_provider);
    if let Some(ttl) = auth.access_token_ttl_secs {
        push("AUTH_ACCESS_TOKEN_TTL_SECS", ttl.to_string());
    }
    if let Some(ttl) = auth.refresh_token_ttl_secs {
        push("AUTH_REFRESH_TOKEN_TTL_SECS", ttl.to_string());
    }
    if let Some(ttl) = auth.auth_code_ttl_secs {
        push("AUTH_CODE_TTL_SECS", ttl.to_string());
    }
    if let Some(rpm) = auth.register_rpm {
        push("AUTH_REGISTER_REQUESTS_PER_MINUTE", rpm.to_string());
    }
    if let Some(rpm) = auth.authorize_rpm {
        push("AUTH_AUTHORIZE_REQUESTS_PER_MINUTE", rpm.to_string());
    }
    if let Some(max) = auth.max_pending_oauth_states {
        push("AUTH_MAX_PENDING_OAUTH_STATES", max.to_string());
    }
    push_opt(
        &mut push,
        "TOKEN_ENCRYPTION_KEY",
        &auth.token_encryption_key,
    );
    vars
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
