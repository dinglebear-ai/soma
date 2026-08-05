//! Environment-variable adapter for typed auth configuration.

use std::time::Duration;

use crate::at_rest::TokenEncryptionKey;
use crate::error::AuthError;

use super::{
    AuthConfig, AuthConfigBuilder, AuthMode, env_key, infer_default_provider, normalize, read_csv,
    read_json, read_path, read_string, read_u32, read_u64, read_url, read_usize,
};

/// Overlays env-style key/value pairs onto a typed AuthConfigBuilder.
///
/// This adapter never reads process environment variables itself. Callers may
/// pass std::env::vars() or values produced from TOML, a secret manager, CLI
/// flags, or another configuration source.
#[derive(Clone, Debug)]
pub struct EnvAuthConfigLoader {
    config: AuthConfig,
}

impl EnvAuthConfigLoader {
    #[must_use]
    pub fn new(builder: AuthConfigBuilder) -> Self {
        Self {
            config: builder.into_config(),
        }
    }

    pub fn load(
        self,
        vars: impl IntoIterator<Item = (String, String)>,
    ) -> Result<AuthConfig, AuthError> {
        let vars = normalize(vars);
        let mut config = self.config;
        let prefix = config.env_prefix.clone();
        let key_mode = env_key(&prefix, "AUTH_MODE");
        let key_admin = env_key(&prefix, "AUTH_ADMIN_EMAIL");
        let key_public_url = env_key(&prefix, "PUBLIC_URL");
        let key_db = env_key(&prefix, "AUTH_SQLITE_PATH");
        let key_keypath = env_key(&prefix, "AUTH_KEY_PATH");
        let key_secret = env_key(&prefix, "AUTH_BOOTSTRAP_SECRET");
        let key_redirects = env_key(&prefix, "AUTH_ALLOWED_REDIRECT_URIS");
        let key_g_id = env_key(&prefix, "GOOGLE_CLIENT_ID");
        let key_g_secret = env_key(&prefix, "GOOGLE_CLIENT_SECRET");
        let key_g_callback = env_key(&prefix, "GOOGLE_CALLBACK_PATH");
        let key_g_scopes = env_key(&prefix, "GOOGLE_SCOPES");
        let key_a_issuer = env_key(&prefix, "AUTHELIA_ISSUER_URL");
        let key_a_id = env_key(&prefix, "AUTHELIA_CLIENT_ID");
        let key_a_secret = env_key(&prefix, "AUTHELIA_CLIENT_SECRET");
        let key_a_callback = env_key(&prefix, "AUTHELIA_CALLBACK_PATH");
        let key_a_scopes = env_key(&prefix, "AUTHELIA_SCOPES");
        let key_gh_id = env_key(&prefix, "GITHUB_CLIENT_ID");
        let key_gh_secret = env_key(&prefix, "GITHUB_CLIENT_SECRET");
        let key_gh_callback = env_key(&prefix, "GITHUB_CALLBACK_PATH");
        let key_gh_scopes = env_key(&prefix, "GITHUB_SCOPES");
        let key_default_provider = env_key(&prefix, "AUTH_DEFAULT_PROVIDER");
        let key_at_ttl = env_key(&prefix, "AUTH_ACCESS_TOKEN_TTL_SECS");
        let key_rt_ttl = env_key(&prefix, "AUTH_REFRESH_TOKEN_TTL_SECS");
        let key_code_ttl = env_key(&prefix, "AUTH_CODE_TTL_SECS");
        let key_reg_rpm = env_key(&prefix, "AUTH_REGISTER_REQUESTS_PER_MINUTE");
        let key_az_rpm = env_key(&prefix, "AUTH_AUTHORIZE_REQUESTS_PER_MINUTE");
        let key_token_rpm = env_key(&prefix, "AUTH_TOKEN_REQUESTS_PER_MINUTE");
        let key_max_pending = env_key(&prefix, "AUTH_MAX_PENDING_OAUTH_STATES");
        let key_enc_key = env_key(&prefix, "TOKEN_ENCRYPTION_KEY");
        let key_machine_clients = env_key(&prefix, "AUTH_MACHINE_CLIENTS_JSON");
        let key_enterprise_issuers = env_key(&prefix, "AUTH_ENTERPRISE_ISSUERS_JSON");

        if vars.contains_key(&key_mode) {
            config.mode = AuthMode::parse(vars.get(&key_mode).map(String::as_str), &key_mode)?;
        }
        if let Some(value) = read_string(&vars, &key_admin) {
            config.admin_email = value.trim().to_ascii_lowercase();
        }
        if vars.contains_key(&key_public_url) {
            config.public_url = read_url(&vars, &key_public_url)?;
        }
        if let Some(path) = read_path(&vars, &key_db) {
            config.sqlite_path = path;
        }
        if let Some(path) = read_path(&vars, &key_keypath) {
            config.key_path = path;
        }
        if let Some(secret) = read_string(&vars, &key_secret) {
            config.bootstrap_secret = Some(secret);
        }
        if let Some(redirects) = read_csv(&vars, &key_redirects) {
            config.allowed_client_redirect_uris = redirects;
        }

        if let Some(value) = read_string(&vars, &key_g_id) {
            config.google.client_id = value;
        }
        if let Some(value) = read_string(&vars, &key_g_secret) {
            config.google.client_secret = value;
        }
        if let Some(value) = read_string(&vars, &key_g_callback) {
            config.google.callback_path = value;
        }
        if let Some(value) = read_csv(&vars, &key_g_scopes) {
            config.google.scopes = value;
        }

        if vars.contains_key(&key_a_issuer) {
            config.authelia.issuer_url = read_url(&vars, &key_a_issuer)?;
        }
        if let Some(value) = read_string(&vars, &key_a_id) {
            config.authelia.client_id = value;
        }
        if let Some(value) = read_string(&vars, &key_a_secret) {
            config.authelia.client_secret = value;
        }
        if let Some(value) = read_string(&vars, &key_a_callback) {
            config.authelia.callback_path = value;
        }
        if let Some(value) = read_csv(&vars, &key_a_scopes) {
            config.authelia.scopes = value;
        }

        if let Some(value) = read_string(&vars, &key_gh_id) {
            config.github.client_id = value;
        }
        if let Some(value) = read_string(&vars, &key_gh_secret) {
            config.github.client_secret = value;
        }
        if let Some(value) = read_string(&vars, &key_gh_callback) {
            config.github.callback_path = value;
        }
        if let Some(value) = read_csv(&vars, &key_gh_scopes) {
            config.github.scopes = value;
        }

        if let Some(value) = read_string(&vars, &key_default_provider) {
            config.default_provider = value.trim().to_ascii_lowercase();
        }
        if let Some(value) = read_u64(&vars, &key_at_ttl)? {
            config.access_token_ttl = Duration::from_secs(value);
        }
        if let Some(value) = read_u64(&vars, &key_rt_ttl)? {
            config.refresh_token_ttl = Duration::from_secs(value);
        }
        if let Some(value) = read_u64(&vars, &key_code_ttl)? {
            config.auth_code_ttl = Duration::from_secs(value);
        }
        if let Some(value) = read_u32(&vars, &key_reg_rpm)? {
            config.register_requests_per_minute = value;
        }
        if let Some(value) = read_u32(&vars, &key_az_rpm)? {
            config.authorize_requests_per_minute = value;
        }
        if let Some(value) = read_u32(&vars, &key_token_rpm)? {
            config.token_requests_per_minute = value;
        }
        if let Some(value) = read_usize(&vars, &key_max_pending)? {
            config.max_pending_oauth_states = value;
        }
        if let Some(raw) = read_string(&vars, &key_enc_key) {
            config.token_encryption_key =
                Some(TokenEncryptionKey::from_encoded(&raw).map_err(|error| {
                    AuthError::Config(format!("invalid {key_enc_key}: {error}"))
                })?);
        }
        if let Some(value) = read_json(&vars, &key_machine_clients)? {
            config.machine_clients = value;
        }
        if let Some(value) = read_json(&vars, &key_enterprise_issuers)? {
            config.enterprise_issuers = value;
        }

        infer_default_provider(&mut config);
        config.validate()?;
        Ok(config)
    }
}
