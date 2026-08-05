use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::at_rest::TokenEncryptionKey;
use crate::error::AuthError;

#[path = "config_env.rs"]
mod config_env;
#[path = "config_machine_clients.rs"]
mod config_machine_clients;
#[path = "config_profile.rs"]
mod config_profile;
#[path = "config_providers.rs"]
mod config_providers;

pub use config_env::EnvAuthConfigLoader;
pub use config_machine_clients::{EnterpriseIssuerConfig, MachineClientConfig};
pub use config_profile::{
    AuthProfile, DEFAULT_ADMIN_SCOPE, DEFAULT_DATA_DIR, DEFAULT_ENV_PREFIX, DEFAULT_LOGIN_PATH,
    DEFAULT_RESOURCE_PATH, DEFAULT_SCOPE, DEFAULT_SESSION_COOKIE_NAME,
    DEFAULT_UPSTREAM_CALLBACK_PATH, DEFAULT_UPSTREAM_CLIENT_NAME,
};
pub use config_providers::{AutheliaConfig, GitHubConfig, GoogleConfig};

const DEFAULT_CALLBACK_PATH: &str = "/auth/google/callback";
const DEFAULT_AUTH_DB_NAME: &str = "auth.db";
const DEFAULT_KEY_NAME: &str = "auth-jwt.pem";
const DEFAULT_ACCESS_TOKEN_TTL_SECS: u64 = 3600;
const DEFAULT_REFRESH_TOKEN_TTL_SECS: u64 = 30 * 24 * 3600;
const DEFAULT_AUTH_CODE_TTL_SECS: u64 = 300;
const DEFAULT_REGISTER_REQUESTS_PER_MINUTE: u32 = 20;
const DEFAULT_AUTHORIZE_REQUESTS_PER_MINUTE: u32 = 60;
const DEFAULT_TOKEN_REQUESTS_PER_MINUTE: u32 = 120;
const DEFAULT_MAX_PENDING_OAUTH_STATES: usize = 1024;

/// This crate's own fixed, non-configurable routes (see `routes.rs::router`).
/// A configured provider `callback_path` colliding with any of these would
/// make axum's route-registration hit its duplicate-route panic at startup
/// — the same failure mode the pairwise provider-vs-provider collision check
/// above guards against, just for a different pair of colliding paths.
const FIXED_ROUTE_PATHS: &[&str] = &[
    "/authorize",
    "/token",
    "/revoke",
    "/jwks",
    "/auth/login",
    "/native/callback",
    "/native/poll",
    "/register",
];
/// Prefix covering every `/.well-known/oauth-*` metadata route, including
/// the `{*route}` wildcard variant.
const WELL_KNOWN_PREFIX: &str = "/.well-known/";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    #[default]
    Bearer,
    OAuth,
}

impl AuthMode {
    fn parse(value: Option<&str>, env_key_for_diagnostics: &str) -> Result<Self, AuthError> {
        match value
            .unwrap_or("bearer")
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "bearer" => Ok(Self::Bearer),
            "oauth" => Ok(Self::OAuth),
            other => Err(AuthError::Config(format!(
                "{env_key_for_diagnostics} must be `bearer` or `oauth`, got `{other}`"
            ))),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthModeConfig {
    pub mode: AuthMode,
}

impl AuthModeConfig {
    pub fn from_sources(
        vars: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, AuthError> {
        Self::from_sources_with_prefix(vars, DEFAULT_ENV_PREFIX)
    }

    pub fn from_sources_with_prefix(
        vars: impl IntoIterator<Item = (String, String)>,
        env_prefix: &str,
    ) -> Result<Self, AuthError> {
        let vars = normalize(vars);
        let key = env_key(env_prefix, "AUTH_MODE");
        Ok(Self {
            mode: AuthMode::parse(vars.get(&key).map(String::as_str), &key)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthConfig {
    pub mode: AuthMode,
    pub public_url: Option<Url>,
    pub sqlite_path: PathBuf,
    pub key_path: PathBuf,
    pub bootstrap_secret: Option<String>,
    pub allowed_client_redirect_uris: Vec<String>,
    /// Single bootstrap admin email permitted to log in through any configured
    /// OAuth/OIDC provider.
    /// Required when `mode == AuthMode::OAuth`. Additional users are granted
    /// through the SQLite-backed allowlist managed via the web UI.
    pub admin_email: String,
    pub google: GoogleConfig,
    pub authelia: AutheliaConfig,
    pub github: GitHubConfig,
    /// Which configured provider `/authorize` and `/auth/login` use when the
    /// request omits `?provider=`. Must name a provider that is actually
    /// configured (validated in `AuthConfig::validate`). Resolved
    /// automatically when unset: `google` > `authelia` > `github`, in that
    /// priority order, picking the first one that has credentials — this is
    /// what makes every existing single-provider (Google-only) deployment
    /// keep working with zero config changes after upgrading.
    pub default_provider: String,
    pub access_token_ttl: Duration,
    pub refresh_token_ttl: Duration,
    pub auth_code_ttl: Duration,
    pub register_requests_per_minute: u32,
    pub authorize_requests_per_minute: u32,
    pub token_requests_per_minute: u32,
    pub max_pending_oauth_states: usize,

    // ---- Brand / consumer-specific parameterization (see L1 bead) ----
    /// Env var prefix used for diagnostics (e.g. `"APP"`, `"AXON"`).
    /// Set via [`AuthConfigBuilder::env_prefix`] BEFORE any env reads.
    pub env_prefix: String,
    /// Default base directory for `auth.db` and `auth-jwt.pem` when the
    /// corresponding env vars are unset.
    pub default_data_dir: PathBuf,
    /// Browser session cookie name supplied by the consumer profile.
    pub session_cookie_name: String,
    /// Scopes advertised on `/.well-known/oauth-authorization-server` and
    /// `/.well-known/oauth-protected-resource`.
    pub scopes_supported: Vec<String>,
    /// Path appended to `public_url` to form the canonical resource URL
    /// returned in the protected-resource metadata document.
    pub resource_path: String,
    /// Default scope applied when `/authorize` requests omit one and the
    /// only scope accepted by the legacy single-scope validator.
    pub default_scope: String,
    /// Scopes minted into the static-bearer-derived `AuthContext`.
    pub static_token_scopes: Vec<String>,
    /// Path of the browser login route (typically `/auth/login`).
    pub login_path: String,
    /// Whether `POST /register` (RFC 7591 dynamic client registration) is
    /// mounted. Defaults to `false` (closed) — opt-in per consumer.
    pub enable_dynamic_registration: bool,
    /// When `true`, dual-mode middleware rejects the static bearer token
    /// whenever OAuth is active. Defaults to `false`; security-sensitive
    /// consumers should opt in explicitly.
    pub disable_static_token_with_oauth: bool,
    /// Client name sent to upstream authorization servers during dynamic registration.
    pub upstream_client_name: String,
    /// Path appended to `public_url` for upstream OAuth authorization callbacks.
    pub upstream_callback_path: String,
    /// Optional at-rest encryption key for upstream provider refresh tokens.
    ///
    /// When present, provider refresh tokens are encrypted with
    /// ChaCha20-Poly1305 before being written to SQLite.  Set via
    /// `{PREFIX}_TOKEN_ENCRYPTION_KEY` (64 hex digits or 43 base64url chars).
    /// When absent, tokens are stored as plaintext (backward-compatible).
    pub token_encryption_key: Option<TokenEncryptionKey>,
    /// Out-of-band machine identities authorized for OAuth client credentials.
    pub machine_clients: Vec<MachineClientConfig>,
    /// Trusted enterprise identity providers authorized to issue ID-JAG grants.
    pub enterprise_issuers: Vec<EnterpriseIssuerConfig>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self::from_profile(AuthProfile::default())
    }
}

impl AuthConfig {
    /// Construct typed configuration from a product profile without reading
    /// process environment variables.
    #[must_use]
    pub fn from_profile(profile: AuthProfile) -> Self {
        let base_dir = profile.default_data_dir.clone();
        Self {
            mode: AuthMode::Bearer,
            public_url: None,
            sqlite_path: base_dir.join(DEFAULT_AUTH_DB_NAME),
            key_path: base_dir.join(DEFAULT_KEY_NAME),
            bootstrap_secret: None,
            allowed_client_redirect_uris: Vec::new(),
            admin_email: String::new(),
            google: GoogleConfig::default(),
            authelia: AutheliaConfig::default(),
            github: GitHubConfig::default(),
            default_provider: String::new(),
            access_token_ttl: Duration::from_secs(DEFAULT_ACCESS_TOKEN_TTL_SECS),
            refresh_token_ttl: Duration::from_secs(DEFAULT_REFRESH_TOKEN_TTL_SECS),
            auth_code_ttl: Duration::from_secs(DEFAULT_AUTH_CODE_TTL_SECS),
            register_requests_per_minute: DEFAULT_REGISTER_REQUESTS_PER_MINUTE,
            authorize_requests_per_minute: DEFAULT_AUTHORIZE_REQUESTS_PER_MINUTE,
            token_requests_per_minute: DEFAULT_TOKEN_REQUESTS_PER_MINUTE,
            max_pending_oauth_states: DEFAULT_MAX_PENDING_OAUTH_STATES,
            env_prefix: profile.env_prefix,
            default_data_dir: base_dir,
            session_cookie_name: profile.session_cookie_name,
            scopes_supported: profile.scopes_supported,
            resource_path: profile.resource_path,
            default_scope: profile.default_scope,
            static_token_scopes: profile.static_token_scopes,
            login_path: profile.login_path,
            enable_dynamic_registration: profile.enable_dynamic_registration,
            disable_static_token_with_oauth: profile.disable_static_token_with_oauth,
            upstream_client_name: profile.upstream_client_name,
            upstream_callback_path: profile.upstream_callback_path,
            token_encryption_key: None,
            machine_clients: Vec::new(),
            enterprise_issuers: Vec::new(),
        }
    }

    /// Read env-style key/value pairs using the generic `APP_*` profile.
    pub fn from_sources(
        vars: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, AuthError> {
        AuthConfigBuilder::new().build_from_sources(vars)
    }

    /// Validate a typed configuration before constructing runtime state.
    pub fn validate(&self) -> Result<(), AuthError> {
        let prefix = &self.env_prefix;
        if !self.google.callback_path.starts_with('/') {
            return Err(AuthError::Config(format!(
                "{prefix}_GOOGLE_CALLBACK_PATH must start with `/`, got `{}`",
                self.google.callback_path
            )));
        }

        if !self.resource_path.starts_with('/') {
            return Err(AuthError::Config(format!(
                "resource_path must start with `/`, got `{}`",
                self.resource_path
            )));
        }
        if !self.login_path.starts_with('/') {
            return Err(AuthError::Config(format!(
                "login_path must start with `/`, got `{}`",
                self.login_path
            )));
        }
        if self.upstream_client_name.trim().is_empty() {
            return Err(AuthError::Config(
                "upstream_client_name must not be empty".to_string(),
            ));
        }
        if !self.upstream_callback_path.starts_with('/') {
            return Err(AuthError::Config(format!(
                "upstream_callback_path must start with `/`, got `{}`",
                self.upstream_callback_path
            )));
        }
        if self.session_cookie_name.is_empty() {
            return Err(AuthError::Config(
                "session_cookie_name must not be empty".to_string(),
            ));
        }
        if self.default_scope.is_empty() {
            return Err(AuthError::Config(
                "default_scope must not be empty".to_string(),
            ));
        }
        if self.scopes_supported.is_empty() {
            return Err(AuthError::Config(
                "scopes_supported must contain at least one scope".to_string(),
            ));
        }
        if !self.scopes_supported.contains(&self.default_scope) {
            return Err(AuthError::Config(format!(
                "default_scope `{}` must be listed in scopes_supported",
                self.default_scope
            )));
        }
        for client in &self.machine_clients {
            if client.client_id.trim().is_empty() {
                return Err(AuthError::Config(
                    "machine clients require client_id".to_string(),
                ));
            }
            if client.client_secret.is_some() == client.jwks.is_some() {
                return Err(AuthError::Config(
                    "machine clients require exactly one of client_secret or jwks".to_string(),
                ));
            }
            if client.resources.is_empty() {
                return Err(AuthError::Config(
                    "machine clients require at least one allowed resource".to_string(),
                ));
            }
        }
        for issuer in &self.enterprise_issuers {
            if issuer.issuer.trim().is_empty()
                || (issuer.jwks_uri.is_none() && issuer.jwks.is_none())
            {
                return Err(AuthError::Config(
                    "enterprise issuers require issuer and jwks_uri or jwks".to_string(),
                ));
            }
            if issuer
                .jwks_uri
                .as_ref()
                .is_some_and(|uri| uri.scheme() != "https")
            {
                return Err(AuthError::Config(
                    "enterprise issuer jwks_uri must use https".to_string(),
                ));
            }
        }

        if matches!(self.mode, AuthMode::OAuth) {
            if self.public_url.is_none() {
                return Err(AuthError::Config(format!(
                    "{prefix}_PUBLIC_URL is required when {prefix}_AUTH_MODE=oauth"
                )));
            }

            let google_configured = !self.google.client_id.is_empty();
            let authelia_configured = !self.authelia.client_id.is_empty();
            let github_configured = !self.github.client_id.is_empty();

            if google_configured && self.google.client_secret.is_empty() {
                return Err(AuthError::Config(format!(
                    "{prefix}_GOOGLE_CLIENT_SECRET is required when {prefix}_GOOGLE_CLIENT_ID is set"
                )));
            }
            if authelia_configured {
                if self.authelia.issuer_url.is_none() {
                    return Err(AuthError::Config(format!(
                        "{prefix}_AUTHELIA_ISSUER_URL is required when {prefix}_AUTHELIA_CLIENT_ID is set"
                    )));
                }
                if self.authelia.client_secret.is_empty() {
                    return Err(AuthError::Config(format!(
                        "{prefix}_AUTHELIA_CLIENT_SECRET is required when {prefix}_AUTHELIA_CLIENT_ID is set"
                    )));
                }
                // Google's authorize/token/JWKS endpoints are hardcoded `https://`
                // string constants — no config can downgrade them. Authelia's are
                // entirely operator-supplied, so unlike Google this crate must
                // enforce the scheme itself: a plaintext issuer would send
                // authorization codes, tokens, and `client_secret` (in the token
                // exchange POST body) over the wire unencrypted with no other
                // signal that anything is wrong.
                if let Some(issuer) = self.authelia.issuer_url.as_ref()
                    && issuer.scheme() != "https"
                {
                    return Err(AuthError::Config(format!(
                        "{prefix}_AUTHELIA_ISSUER_URL must use https, got `{}`",
                        issuer.scheme()
                    )));
                }
            }
            if github_configured && self.github.client_secret.is_empty() {
                return Err(AuthError::Config(format!(
                    "{prefix}_GITHUB_CLIENT_SECRET is required when {prefix}_GITHUB_CLIENT_ID is set"
                )));
            }
            // GitHubProvider::exchange_code's GET /user/emails call requires
            // this scope; GitHub returns it in a hard failure (not a graceful
            // `email: None`, unlike Google/Authelia's ID-token-derived email
            // claim), and tokio::try_join! propagates that as a total login
            // failure. Catch the misconfiguration here instead of at runtime.
            if github_configured && !self.github.scopes.iter().any(|scope| scope == "user:email") {
                return Err(AuthError::Config(format!(
                    "{prefix}_GITHUB_SCOPES must include `user:email` (got `{:?}`)",
                    self.github.scopes
                )));
            }
            // Two configured providers with the same (possibly operator-overridden)
            // callback_path would make routes.rs's per-provider route-mounting loop
            // (Task 10) hit axum's duplicate-route panic at startup instead of a
            // clean config-time error — check pairwise uniqueness among only the
            // providers that are actually configured.
            //
            // Compare the NORMALIZED path (leading `/` guaranteed), not the raw
            // config string: `build_provider_redirect_uri` (state.rs) strips any
            // leading `/` from `callback_path` and re-adds exactly one before
            // mounting the route, so an operator-supplied path without a leading
            // `/` (e.g. `authorize`) mounts as `/authorize` at startup even though
            // it wouldn't textually match `/authorize` in `FIXED_ROUTE_PATHS` or
            // another provider's raw `callback_path`. Normalizing here first keeps
            // this check honest about what actually gets mounted.
            {
                fn normalize_callback_path(path: &str) -> String {
                    format!("/{}", path.trim_start_matches('/'))
                }

                let mut configured_paths: Vec<(&str, String)> = Vec::new();
                if google_configured {
                    configured_paths.push((
                        "google",
                        normalize_callback_path(&self.google.callback_path),
                    ));
                }
                if authelia_configured {
                    configured_paths.push((
                        "authelia",
                        normalize_callback_path(&self.authelia.callback_path),
                    ));
                }
                if github_configured {
                    configured_paths.push((
                        "github",
                        normalize_callback_path(&self.github.callback_path),
                    ));
                }
                for i in 0..configured_paths.len() {
                    for j in (i + 1)..configured_paths.len() {
                        if configured_paths[i].1 == configured_paths[j].1 {
                            return Err(AuthError::Config(format!(
                                "{prefix}_{a}_CALLBACK_PATH and {prefix}_{b}_CALLBACK_PATH must not both resolve to `{path}`",
                                a = configured_paths[i].0.to_ascii_uppercase(),
                                b = configured_paths[j].0.to_ascii_uppercase(),
                                path = configured_paths[i].1,
                            )));
                        }
                    }
                }
                // Same failure mode as above, but against this crate's own
                // fixed routes rather than another provider's callback_path.
                for (provider, path) in &configured_paths {
                    if FIXED_ROUTE_PATHS.contains(&path.as_str())
                        || path.starts_with(WELL_KNOWN_PREFIX)
                    {
                        return Err(AuthError::Config(format!(
                            "{prefix}_{provider_upper}_CALLBACK_PATH must not resolve to `{path}` — \
                             that path is reserved for this crate's own `{path}` route",
                            provider_upper = provider.to_ascii_uppercase(),
                        )));
                    }
                }
            }
            if !google_configured && !authelia_configured && !github_configured {
                return Err(AuthError::Config(format!(
                    "at least one OAuth provider must be configured when {prefix}_AUTH_MODE=oauth — \
                     set {prefix}_GOOGLE_CLIENT_ID, {prefix}_AUTHELIA_CLIENT_ID (+ {prefix}_AUTHELIA_ISSUER_URL), \
                     or {prefix}_GITHUB_CLIENT_ID (each paired with its matching _CLIENT_SECRET)"
                )));
            }
            match self.default_provider.as_str() {
                "google" if !google_configured => {
                    return Err(AuthError::Config(format!(
                        "{prefix}_AUTH_DEFAULT_PROVIDER=google but {prefix}_GOOGLE_CLIENT_ID is not set"
                    )));
                }
                "authelia" if !authelia_configured => {
                    return Err(AuthError::Config(format!(
                        "{prefix}_AUTH_DEFAULT_PROVIDER=authelia but {prefix}_AUTHELIA_CLIENT_ID is not set"
                    )));
                }
                "github" if !github_configured => {
                    return Err(AuthError::Config(format!(
                        "{prefix}_AUTH_DEFAULT_PROVIDER=github but {prefix}_GITHUB_CLIENT_ID is not set"
                    )));
                }
                "google" | "authelia" | "github" => {}
                other => {
                    return Err(AuthError::Config(format!(
                        "{prefix}_AUTH_DEFAULT_PROVIDER must be `google`, `authelia`, or `github`, got `{other}`"
                    )));
                }
            }
            if self.admin_email.is_empty() {
                return Err(AuthError::Config(format!(
                    "{prefix}_AUTH_ADMIN_EMAIL is required when {prefix}_AUTH_MODE=oauth — \
                     set the admin's email so no account can log in unless explicitly permitted"
                )));
            }
        }

        Ok(())
    }
}

/// Typed builder for AuthConfig. Environment loading is provided by
/// EnvAuthConfigLoader and AuthConfigBuilder::build_from_sources.
///
/// Applications should start from an explicit AuthProfile at their composition
/// root, then set typed provider and policy fields before calling build.
#[derive(Clone, Debug)]
pub struct AuthConfigBuilder {
    config: AuthConfig,
}

impl Default for AuthConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthConfigBuilder {
    /// Start from the generic, product-neutral profile.
    #[must_use]
    pub fn new() -> Self {
        Self::from_profile(AuthProfile::default())
    }

    /// Start from an application-owned product profile.
    #[must_use]
    pub fn from_profile(profile: AuthProfile) -> Self {
        Self {
            config: AuthConfig::from_profile(profile),
        }
    }

    #[must_use]
    pub const fn mode(mut self, mode: AuthMode) -> Self {
        self.config.mode = mode;
        self
    }

    #[must_use]
    pub fn public_url(mut self, url: Url) -> Self {
        self.config.public_url = Some(url);
        self
    }

    #[must_use]
    pub fn sqlite_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.sqlite_path = path.into();
        self
    }

    #[must_use]
    pub fn key_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.key_path = path.into();
        self
    }

    #[must_use]
    pub fn bootstrap_secret(mut self, secret: impl Into<String>) -> Self {
        self.config.bootstrap_secret = Some(secret.into());
        self
    }

    #[must_use]
    pub fn allowed_client_redirect_uris(mut self, uris: Vec<String>) -> Self {
        self.config.allowed_client_redirect_uris = uris;
        self
    }

    #[must_use]
    pub fn admin_email(mut self, email: impl Into<String>) -> Self {
        self.config.admin_email = email.into().trim().to_ascii_lowercase();
        self
    }

    #[must_use]
    pub fn google(mut self, config: GoogleConfig) -> Self {
        self.config.google = config;
        self
    }

    #[must_use]
    pub fn authelia(mut self, config: AutheliaConfig) -> Self {
        self.config.authelia = config;
        self
    }

    #[must_use]
    pub fn github(mut self, config: GitHubConfig) -> Self {
        self.config.github = config;
        self
    }

    #[must_use]
    pub fn default_provider(mut self, provider: impl Into<String>) -> Self {
        self.config.default_provider = provider.into().trim().to_ascii_lowercase();
        self
    }

    #[must_use]
    pub const fn access_token_ttl(mut self, ttl: Duration) -> Self {
        self.config.access_token_ttl = ttl;
        self
    }

    #[must_use]
    pub const fn refresh_token_ttl(mut self, ttl: Duration) -> Self {
        self.config.refresh_token_ttl = ttl;
        self
    }

    #[must_use]
    pub const fn auth_code_ttl(mut self, ttl: Duration) -> Self {
        self.config.auth_code_ttl = ttl;
        self
    }

    #[must_use]
    pub const fn register_requests_per_minute(mut self, limit: u32) -> Self {
        self.config.register_requests_per_minute = limit;
        self
    }

    #[must_use]
    pub const fn authorize_requests_per_minute(mut self, limit: u32) -> Self {
        self.config.authorize_requests_per_minute = limit;
        self
    }

    #[must_use]
    pub const fn token_requests_per_minute(mut self, limit: u32) -> Self {
        self.config.token_requests_per_minute = limit;
        self
    }

    #[must_use]
    pub const fn max_pending_oauth_states(mut self, limit: usize) -> Self {
        self.config.max_pending_oauth_states = limit;
        self
    }

    #[must_use]
    pub fn env_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.config.env_prefix = prefix.into();
        self
    }

    #[must_use]
    pub fn default_data_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        self.config.sqlite_path = dir.join(DEFAULT_AUTH_DB_NAME);
        self.config.key_path = dir.join(DEFAULT_KEY_NAME);
        self.config.default_data_dir = dir;
        self
    }

    #[must_use]
    pub fn session_cookie_name(mut self, name: impl Into<String>) -> Self {
        self.config.session_cookie_name = name.into();
        self
    }

    #[must_use]
    pub fn scopes_supported(mut self, scopes: Vec<String>) -> Self {
        self.config.scopes_supported = scopes;
        self
    }

    #[must_use]
    pub fn resource_path(mut self, path: impl Into<String>) -> Self {
        self.config.resource_path = path.into();
        self
    }

    #[must_use]
    pub fn default_scope(mut self, scope: impl Into<String>) -> Self {
        self.config.default_scope = scope.into();
        self
    }

    #[must_use]
    pub fn static_token_scopes(mut self, scopes: Vec<String>) -> Self {
        self.config.static_token_scopes = scopes;
        self
    }

    #[must_use]
    pub fn login_path(mut self, path: impl Into<String>) -> Self {
        self.config.login_path = path.into();
        self
    }

    #[must_use]
    pub const fn enable_dynamic_registration(mut self, enabled: bool) -> Self {
        self.config.enable_dynamic_registration = enabled;
        self
    }

    #[must_use]
    pub const fn disable_static_token_with_oauth(mut self, disabled: bool) -> Self {
        self.config.disable_static_token_with_oauth = disabled;
        self
    }

    #[must_use]
    pub fn upstream_client_name(mut self, name: impl Into<String>) -> Self {
        self.config.upstream_client_name = name.into();
        self
    }

    #[must_use]
    pub fn upstream_callback_path(mut self, path: impl Into<String>) -> Self {
        self.config.upstream_callback_path = path.into();
        self
    }

    #[must_use]
    pub fn token_encryption_key(mut self, key: TokenEncryptionKey) -> Self {
        self.config.token_encryption_key = Some(key);
        self
    }

    #[must_use]
    pub fn machine_clients(mut self, clients: Vec<MachineClientConfig>) -> Self {
        self.config.machine_clients = clients;
        self
    }

    #[must_use]
    pub fn enterprise_issuers(mut self, issuers: Vec<EnterpriseIssuerConfig>) -> Self {
        self.config.enterprise_issuers = issuers;
        self
    }

    /// Validate and return typed configuration without reading environment variables.
    pub fn build(self) -> Result<AuthConfig, AuthError> {
        let mut config = self.config;
        infer_default_provider(&mut config);
        config.validate()?;
        Ok(config)
    }

    /// Overlay supplied env-style key/value pairs, then validate the result.
    pub fn build_from_sources(
        self,
        vars: impl IntoIterator<Item = (String, String)>,
    ) -> Result<AuthConfig, AuthError> {
        EnvAuthConfigLoader::new(self).load(vars)
    }

    pub(crate) fn into_config(self) -> AuthConfig {
        self.config
    }
}

fn infer_default_provider(config: &mut AuthConfig) {
    if !config.default_provider.trim().is_empty() {
        return;
    }
    config.default_provider = if !config.google.client_id.is_empty() {
        "google"
    } else if !config.authelia.client_id.is_empty() {
        "authelia"
    } else if !config.github.client_id.is_empty() {
        "github"
    } else {
        "google"
    }
    .to_string();
}

fn env_key(prefix: &str, suffix: &str) -> String {
    let trimmed = prefix.trim_end_matches('_');
    if trimmed.is_empty() {
        suffix.to_string()
    } else {
        format!("{trimmed}_{suffix}")
    }
}

fn normalize(vars: impl IntoIterator<Item = (String, String)>) -> HashMap<String, String> {
    vars.into_iter()
        .filter_map(|(key, value)| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some((key, trimmed.to_string()))
            }
        })
        .collect()
}

fn read_string(vars: &HashMap<String, String>, key: &str) -> Option<String> {
    vars.get(key).cloned()
}

fn read_path(vars: &HashMap<String, String>, key: &str) -> Option<PathBuf> {
    read_string(vars, key).map(PathBuf::from)
}

fn read_csv(vars: &HashMap<String, String>, key: &str) -> Option<Vec<String>> {
    read_string(vars, key).map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    })
}

fn read_json<T: serde::de::DeserializeOwned>(
    vars: &HashMap<String, String>,
    key: &str,
) -> Result<Option<T>, AuthError> {
    read_string(vars, key)
        .map(|value| {
            serde_json::from_str(&value)
                .map_err(|error| AuthError::Config(format!("{key} must be valid JSON: {error}")))
        })
        .transpose()
}

fn read_url(vars: &HashMap<String, String>, key: &str) -> Result<Option<Url>, AuthError> {
    read_string(vars, key)
        .map(|value| {
            Url::parse(&value)
                .map_err(|error| AuthError::Config(format!("{key} must be a valid URL: {error}")))
        })
        .transpose()
}

fn read_u64(vars: &HashMap<String, String>, key: &str) -> Result<Option<u64>, AuthError> {
    read_string(vars, key)
        .map(|value| {
            value.parse::<u64>().map_err(|error| {
                AuthError::Config(format!(
                    "{key} must be an integer number of seconds: {error}"
                ))
            })
        })
        .transpose()
}

fn read_u32(vars: &HashMap<String, String>, key: &str) -> Result<Option<u32>, AuthError> {
    read_string(vars, key)
        .map(|value| {
            value.parse::<u32>().map_err(|error| {
                AuthError::Config(format!(
                    "{key} must be an integer number of requests per minute: {error}"
                ))
            })
        })
        .transpose()
}

fn read_usize(vars: &HashMap<String, String>, key: &str) -> Result<Option<usize>, AuthError> {
    read_string(vars, key)
        .map(|value| {
            value.parse::<usize>().map_err(|error| {
                AuthError::Config(format!("{key} must be a positive integer: {error}"))
            })
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::{
        AuthConfig, AuthConfigBuilder, AuthMode, AuthModeConfig, AuthProfile, AutheliaConfig,
    };

    /// Guards against a regression where `GoogleConfig`/`AutheliaConfig`/
    /// `GitHubConfig` derived `Default` (giving `callback_path: String::new()`
    /// instead of the `#[serde(default = "fn")]` value) made `validate()`'s
    /// unconditional Google callback-path check reject ANY struct-literal
    /// `AuthConfig` that configures only Authelia/GitHub and relies on
    /// `..AuthConfig::default()` for the unused `google` field — a shape that
    /// bypasses `AuthConfigBuilder` entirely (test fixtures, or a downstream
    /// consumer constructing `AuthConfig` directly).
    #[test]
    fn validate_accepts_a_struct_literal_config_configuring_only_authelia() {
        let cfg = AuthConfig {
            mode: AuthMode::OAuth,
            public_url: Some(url::Url::parse("https://app.example.com").unwrap()),
            admin_email: "admin@example.com".to_string(),
            authelia: AutheliaConfig {
                issuer_url: Some(url::Url::parse("https://auth.example.com").unwrap()),
                client_id: "id".to_string(),
                client_secret: "secret".to_string(),
                ..AutheliaConfig::default()
            },
            default_provider: "authelia".to_string(),
            ..AuthConfig::default()
        };
        cfg.validate().expect(
            "google's untouched defaults must not block validation of an authelia-only config",
        );
    }

    #[test]
    fn bearer_mode_preserves_existing_http_token_behavior() {
        let cfg = AuthModeConfig::from_sources(fake_env_with("APP_AUTH_MODE", "bearer")).unwrap();
        assert!(matches!(cfg.mode, AuthMode::Bearer));
    }

    #[test]
    fn oauth_mode_requires_public_url_and_google_credentials() {
        let err = AuthConfig::from_sources(fake_env_with_many([
            ("APP_AUTH_MODE", "oauth"),
            ("APP_GOOGLE_CLIENT_ID", "id"),
        ]))
        .unwrap_err();
        assert!(err.to_string().contains("APP_PUBLIC_URL"));
    }

    #[test]
    fn oauth_mode_requires_at_least_one_configured_provider() {
        let err = AuthConfig::from_sources(fake_env_with_many([
            ("APP_AUTH_MODE", "oauth"),
            ("APP_PUBLIC_URL", "https://app.example.com"),
            ("APP_AUTH_ADMIN_EMAIL", "admin@example.com"),
        ]))
        .unwrap_err();
        assert!(err.to_string().contains("at least one OAuth provider"));
    }

    #[test]
    fn oauth_mode_accepts_authelia_only_configuration() {
        let cfg = AuthConfig::from_sources(fake_env_with_many([
            ("APP_AUTH_MODE", "oauth"),
            ("APP_PUBLIC_URL", "https://app.example.com"),
            ("APP_AUTHELIA_ISSUER_URL", "https://auth.example.com"),
            ("APP_AUTHELIA_CLIENT_ID", "id"),
            ("APP_AUTHELIA_CLIENT_SECRET", "secret"),
            ("APP_AUTH_ADMIN_EMAIL", "admin@example.com"),
        ]))
        .unwrap();
        assert_eq!(cfg.default_provider, "authelia");
    }

    #[test]
    fn oauth_mode_accepts_github_only_configuration() {
        let cfg = AuthConfig::from_sources(fake_env_with_many([
            ("APP_AUTH_MODE", "oauth"),
            ("APP_PUBLIC_URL", "https://app.example.com"),
            ("APP_GITHUB_CLIENT_ID", "id"),
            ("APP_GITHUB_CLIENT_SECRET", "secret"),
            ("APP_AUTH_ADMIN_EMAIL", "admin@example.com"),
        ]))
        .unwrap();
        assert_eq!(cfg.default_provider, "github");
    }

    #[test]
    fn oauth_mode_rejects_github_scopes_missing_user_email() {
        let err = AuthConfig::from_sources(fake_env_with_many([
            ("APP_AUTH_MODE", "oauth"),
            ("APP_PUBLIC_URL", "https://app.example.com"),
            ("APP_GITHUB_CLIENT_ID", "id"),
            ("APP_GITHUB_CLIENT_SECRET", "secret"),
            ("APP_GITHUB_SCOPES", "read:user"),
            ("APP_AUTH_ADMIN_EMAIL", "admin@example.com"),
        ]))
        .unwrap_err();
        assert!(err.to_string().contains("user:email"));
    }

    #[test]
    fn oauth_mode_default_provider_prefers_google_when_multiple_are_configured() {
        let cfg = AuthConfig::from_sources(fake_env_with_many([
            ("APP_AUTH_MODE", "oauth"),
            ("APP_PUBLIC_URL", "https://app.example.com"),
            ("APP_GOOGLE_CLIENT_ID", "id"),
            ("APP_GOOGLE_CLIENT_SECRET", "secret"),
            ("APP_GITHUB_CLIENT_ID", "gh-id"),
            ("APP_GITHUB_CLIENT_SECRET", "gh-secret"),
            ("APP_AUTH_ADMIN_EMAIL", "admin@example.com"),
        ]))
        .unwrap();
        assert_eq!(cfg.default_provider, "google");
    }

    #[test]
    fn oauth_mode_rejects_default_provider_naming_an_unconfigured_provider() {
        let err = AuthConfig::from_sources(fake_env_with_many([
            ("APP_AUTH_MODE", "oauth"),
            ("APP_PUBLIC_URL", "https://app.example.com"),
            ("APP_GOOGLE_CLIENT_ID", "id"),
            ("APP_GOOGLE_CLIENT_SECRET", "secret"),
            ("APP_AUTH_ADMIN_EMAIL", "admin@example.com"),
            ("APP_AUTH_DEFAULT_PROVIDER", "github"),
        ]))
        .unwrap_err();
        assert!(err.to_string().contains("APP_AUTH_DEFAULT_PROVIDER=github"));
    }

    #[test]
    fn oauth_mode_rejects_a_non_https_authelia_issuer_url() {
        let err = AuthConfig::from_sources(fake_env_with_many([
            ("APP_AUTH_MODE", "oauth"),
            ("APP_PUBLIC_URL", "https://app.example.com"),
            ("APP_AUTHELIA_ISSUER_URL", "http://auth.internal"),
            ("APP_AUTHELIA_CLIENT_ID", "id"),
            ("APP_AUTHELIA_CLIENT_SECRET", "secret"),
            ("APP_AUTH_ADMIN_EMAIL", "admin@example.com"),
        ]))
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("APP_AUTHELIA_ISSUER_URL must use https")
        );
    }

    #[test]
    fn oauth_mode_rejects_two_configured_providers_sharing_a_callback_path() {
        let err = AuthConfig::from_sources(fake_env_with_many([
            ("APP_AUTH_MODE", "oauth"),
            ("APP_PUBLIC_URL", "https://app.example.com"),
            ("APP_GOOGLE_CLIENT_ID", "id"),
            ("APP_GOOGLE_CLIENT_SECRET", "secret"),
            ("APP_GITHUB_CLIENT_ID", "gh-id"),
            ("APP_GITHUB_CLIENT_SECRET", "gh-secret"),
            ("APP_GITHUB_CALLBACK_PATH", "/auth/google/callback"),
            ("APP_AUTH_ADMIN_EMAIL", "admin@example.com"),
        ]))
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("must not both resolve to `/auth/google/callback`")
        );
    }

    #[test]
    fn oauth_mode_rejects_two_configured_providers_sharing_a_callback_path_missing_a_leading_slash()
    {
        // A `callback_path` without a leading `/` still mounts at the same
        // normalized route as one that has it (build_provider_redirect_uri
        // in state.rs prepends the missing `/`), so the collision check must
        // catch this even though the raw strings don't textually match.
        let err = AuthConfig::from_sources(fake_env_with_many([
            ("APP_AUTH_MODE", "oauth"),
            ("APP_PUBLIC_URL", "https://app.example.com"),
            ("APP_GOOGLE_CLIENT_ID", "id"),
            ("APP_GOOGLE_CLIENT_SECRET", "secret"),
            ("APP_GITHUB_CLIENT_ID", "gh-id"),
            ("APP_GITHUB_CLIENT_SECRET", "gh-secret"),
            ("APP_GITHUB_CALLBACK_PATH", "auth/google/callback"),
            ("APP_AUTH_ADMIN_EMAIL", "admin@example.com"),
        ]))
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("must not both resolve to `/auth/google/callback`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn oauth_mode_rejects_a_callback_path_colliding_with_a_fixed_crate_route() {
        let err = AuthConfig::from_sources(fake_env_with_many([
            ("APP_AUTH_MODE", "oauth"),
            ("APP_PUBLIC_URL", "https://app.example.com"),
            ("APP_GOOGLE_CLIENT_ID", "id"),
            ("APP_GOOGLE_CLIENT_SECRET", "secret"),
            ("APP_GOOGLE_CALLBACK_PATH", "/authorize"),
            ("APP_AUTH_ADMIN_EMAIL", "admin@example.com"),
        ]))
        .unwrap_err();
        assert!(
            err.to_string().contains("must not resolve to `/authorize`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn oauth_mode_rejects_a_callback_path_colliding_with_a_fixed_crate_route_missing_a_leading_slash()
     {
        // Same as above but without the leading `/` on the operator-supplied
        // value. Uses GitHub, not Google: Google's callback_path has its own
        // unconditional "must start with `/`" check earlier in validate()
        // (a different guard than the one under test here), so a Google
        // fixture would never reach the collision-check normalization this
        // test exists to cover. GitHub/Authelia have no such standalone
        // check, so this is the only path that exercises it — the value
        // still mounts at `/authorize` once state.rs builds the redirect URI.
        let err = AuthConfig::from_sources(fake_env_with_many([
            ("APP_AUTH_MODE", "oauth"),
            ("APP_PUBLIC_URL", "https://app.example.com"),
            ("APP_GITHUB_CLIENT_ID", "id"),
            ("APP_GITHUB_CLIENT_SECRET", "secret"),
            ("APP_GITHUB_CALLBACK_PATH", "authorize"),
            ("APP_AUTH_ADMIN_EMAIL", "admin@example.com"),
        ]))
        .unwrap_err();
        assert!(
            err.to_string().contains("must not resolve to `/authorize`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn oauth_mode_rejects_a_callback_path_under_the_well_known_prefix() {
        let err = AuthConfig::from_sources(fake_env_with_many([
            ("APP_AUTH_MODE", "oauth"),
            ("APP_PUBLIC_URL", "https://app.example.com"),
            ("APP_GOOGLE_CLIENT_ID", "id"),
            ("APP_GOOGLE_CLIENT_SECRET", "secret"),
            (
                "APP_GOOGLE_CALLBACK_PATH",
                "/.well-known/oauth-authorization-server",
            ),
            ("APP_AUTH_ADMIN_EMAIL", "admin@example.com"),
        ]))
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("must not resolve to `/.well-known/oauth-authorization-server`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn oauth_mode_defaults_paths_and_callback() {
        let cfg = AuthConfig::from_sources(fake_env_with_many([
            ("APP_AUTH_MODE", "oauth"),
            ("APP_PUBLIC_URL", "https://app.example.com"),
            ("APP_GOOGLE_CLIENT_ID", "id"),
            ("APP_GOOGLE_CLIENT_SECRET", "secret"),
            ("APP_AUTH_ADMIN_EMAIL", "admin@example.com"),
        ]))
        .unwrap();
        assert_eq!(cfg.sqlite_path.file_name().unwrap(), "auth.db");
        assert_eq!(cfg.key_path.file_name().unwrap(), "auth-jwt.pem");
        assert_eq!(cfg.google.callback_path, "/auth/google/callback");
    }

    #[test]
    fn oauth_mode_requires_admin_email() {
        let err = AuthConfig::from_sources(fake_env_with_many([
            ("APP_AUTH_MODE", "oauth"),
            ("APP_PUBLIC_URL", "https://app.example.com"),
            ("APP_GOOGLE_CLIENT_ID", "id"),
            ("APP_GOOGLE_CLIENT_SECRET", "secret"),
        ]))
        .unwrap_err();
        assert!(err.to_string().contains("APP_AUTH_ADMIN_EMAIL"));
    }

    #[test]
    fn admin_email_normalizes_case_and_trims_whitespace() {
        let cfg = AuthConfig::from_sources(fake_env_with_many([
            ("APP_AUTH_MODE", "oauth"),
            ("APP_PUBLIC_URL", "https://app.example.com"),
            ("APP_GOOGLE_CLIENT_ID", "id"),
            ("APP_GOOGLE_CLIENT_SECRET", "secret"),
            ("APP_AUTH_ADMIN_EMAIL", "  Admin@Example.COM  "),
        ]))
        .unwrap();
        assert_eq!(cfg.admin_email, "admin@example.com");
    }

    #[test]
    fn oauth_mode_parses_allowed_client_redirect_uris() {
        let cfg = AuthConfig::from_sources(fake_env_with_many([
            ("APP_AUTH_MODE", "oauth"),
            ("APP_PUBLIC_URL", "https://app.example.com"),
            ("APP_GOOGLE_CLIENT_ID", "id"),
            ("APP_GOOGLE_CLIENT_SECRET", "secret"),
            ("APP_AUTH_ADMIN_EMAIL", "admin@example.com"),
            (
                "APP_AUTH_ALLOWED_REDIRECT_URIS",
                "https://callback.tootie.tv/callback/*,https://claude.ai/api/mcp/auth_callback",
            ),
        ]))
        .unwrap();
        assert_eq!(
            cfg.allowed_client_redirect_uris,
            vec![
                "https://callback.tootie.tv/callback/*".to_string(),
                "https://claude.ai/api/mcp/auth_callback".to_string()
            ]
        );
    }

    #[test]
    fn default_config_uses_generic_product_profile() {
        let cfg = AuthConfig::default();
        assert_eq!(cfg.env_prefix, "APP");
        assert_eq!(cfg.session_cookie_name, "auth_session");
        assert_eq!(
            cfg.scopes_supported,
            vec!["app:read".to_string(), "app:admin".to_string()]
        );
        assert_eq!(cfg.resource_path, "/mcp");
        assert_eq!(cfg.default_scope, "app:read");
        assert_eq!(
            cfg.static_token_scopes,
            vec!["app:read".to_string(), "app:admin".to_string()]
        );
        assert_eq!(cfg.login_path, "/auth/login");
        assert!(!cfg.enable_dynamic_registration);
        assert!(!cfg.disable_static_token_with_oauth);
        // The generic profile must not resolve the SQLite token store / JWT
        // signing key to a bare relative path (cwd-dependent) whenever the
        // environment offers a platform data dir or home dir to anchor on —
        // see `config_profile::resolve_default_data_dir`.
        if dirs::data_dir().is_none() && dirs::home_dir().is_none() {
            assert_eq!(cfg.default_data_dir, std::path::PathBuf::from(".auth"));
        } else {
            assert_ne!(cfg.default_data_dir, std::path::PathBuf::from(".auth"));
            assert!(cfg.default_data_dir.is_absolute());
        }
        assert_eq!(cfg.upstream_client_name, "app");
        assert_eq!(cfg.upstream_callback_path, "/auth/upstream/callback");
    }

    #[test]
    fn builder_env_prefix_resolves_consumer_env_vars() {
        let cfg = AuthConfigBuilder::new()
            .env_prefix("SYSLOG_MCP")
            .session_cookie_name("syslog_session")
            .scopes_supported(vec!["syslog:read".to_string(), "syslog:admin".to_string()])
            .default_scope("syslog:read")
            .static_token_scopes(vec!["syslog:read".to_string(), "syslog:admin".to_string()])
            .disable_static_token_with_oauth(true)
            .build_from_sources(fake_env_with_many([
                ("SYSLOG_MCP_AUTH_MODE", "oauth"),
                ("SYSLOG_MCP_PUBLIC_URL", "https://syslog.example.com"),
                ("SYSLOG_MCP_GOOGLE_CLIENT_ID", "id"),
                ("SYSLOG_MCP_GOOGLE_CLIENT_SECRET", "secret"),
                ("SYSLOG_MCP_AUTH_ADMIN_EMAIL", "admin@example.com"),
            ]))
            .unwrap();
        assert!(matches!(cfg.mode, AuthMode::OAuth));
        assert_eq!(cfg.env_prefix, "SYSLOG_MCP");
        assert_eq!(cfg.session_cookie_name, "syslog_session");
        assert_eq!(cfg.default_scope, "syslog:read");
        assert!(cfg.disable_static_token_with_oauth);
        assert_eq!(
            cfg.scopes_supported,
            vec!["syslog:read".to_string(), "syslog:admin".to_string()]
        );
    }

    #[test]
    fn builder_unrelated_env_vars_are_ignored_when_prefix_is_overridden() {
        // Vars use APP_*; builder is set to SYSLOG_MCP — so AUTH_MODE goes
        // unread, defaults to bearer, and PUBLIC_URL stays None.
        let cfg = AuthConfigBuilder::new()
            .env_prefix("SYSLOG_MCP")
            .build_from_sources(fake_env_with_many([
                ("APP_AUTH_MODE", "oauth"),
                ("APP_PUBLIC_URL", "https://app.example.com"),
                ("APP_GOOGLE_CLIENT_ID", "id"),
                ("APP_GOOGLE_CLIENT_SECRET", "secret"),
                ("APP_AUTH_ADMIN_EMAIL", "admin@example.com"),
            ]))
            .unwrap();
        assert!(matches!(cfg.mode, AuthMode::Bearer));
        assert!(cfg.public_url.is_none());
    }

    #[test]
    fn builder_validates_resource_path_starts_with_slash() {
        let err = AuthConfigBuilder::new()
            .resource_path("mcp")
            .build_from_sources(Vec::<(String, String)>::new())
            .unwrap_err();
        assert!(err.to_string().contains("resource_path"));
    }

    #[test]
    fn builder_validates_login_path_starts_with_slash() {
        let err = AuthConfigBuilder::new()
            .login_path("auth/login")
            .build_from_sources(Vec::<(String, String)>::new())
            .unwrap_err();
        assert!(err.to_string().contains("login_path"));
    }

    #[test]
    fn typed_builder_builds_without_environment_loading() {
        let profile = AuthProfile {
            env_prefix: "AXON".to_string(),
            default_data_dir: std::path::PathBuf::from("/tmp/axon-auth"),
            session_cookie_name: "axon_session".to_string(),
            scopes_supported: vec!["axon:read".to_string(), "axon:admin".to_string()],
            resource_path: "/mcp".to_string(),
            default_scope: "axon:read".to_string(),
            static_token_scopes: vec!["axon:read".to_string(), "axon:admin".to_string()],
            login_path: "/auth/login".to_string(),
            enable_dynamic_registration: true,
            disable_static_token_with_oauth: true,
            upstream_client_name: "axon".to_string(),
            upstream_callback_path: "/oauth/upstream/callback".to_string(),
        };

        let config = AuthConfigBuilder::from_profile(profile)
            .build()
            .expect("typed profile builds without env");

        assert!(matches!(config.mode, AuthMode::Bearer));
        assert_eq!(config.env_prefix, "AXON");
        assert_eq!(
            config.sqlite_path,
            std::path::PathBuf::from("/tmp/axon-auth/auth.db")
        );
        assert_eq!(config.upstream_client_name, "axon");
        assert_eq!(config.upstream_callback_path, "/oauth/upstream/callback");
        assert!(config.disable_static_token_with_oauth);
    }

    #[test]
    fn env_loader_overlays_only_supplied_values() {
        let config = AuthConfigBuilder::new()
            .session_cookie_name("custom_session")
            .upstream_client_name("custom-client")
            .build_from_sources(fake_env_with("APP_AUTH_ACCESS_TOKEN_TTL_SECS", "99"))
            .expect("env overlay builds");

        assert_eq!(config.access_token_ttl.as_secs(), 99);
        assert_eq!(config.session_cookie_name, "custom_session");
        assert_eq!(config.upstream_client_name, "custom-client");
    }

    #[test]
    fn builder_rejects_invalid_upstream_identity() {
        let empty_name = AuthConfigBuilder::new()
            .upstream_client_name(" ")
            .build()
            .unwrap_err();
        assert!(empty_name.to_string().contains("upstream_client_name"));

        let relative_callback = AuthConfigBuilder::new()
            .upstream_callback_path("oauth/callback")
            .build()
            .unwrap_err();
        assert!(
            relative_callback
                .to_string()
                .contains("upstream_callback_path")
        );
    }

    fn fake_env_with(key: &'static str, value: &'static str) -> Vec<(String, String)> {
        vec![(key.to_string(), value.to_string())]
    }

    fn fake_env_with_many<const N: usize>(
        pairs: [(&'static str, &'static str); N],
    ) -> Vec<(String, String)> {
        pairs
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }
}
