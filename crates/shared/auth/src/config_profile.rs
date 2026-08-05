//! Product identity and generic defaults for the reusable auth engine.

use std::path::PathBuf;

/// Generic env-var prefix used when a consumer does not provide a product profile.
pub const DEFAULT_ENV_PREFIX: &str = "APP";
/// Generic browser session cookie name.
pub const DEFAULT_SESSION_COOKIE_NAME: &str = "auth_session";
/// Generic default OAuth scope.
pub const DEFAULT_SCOPE: &str = "app:read";
/// Generic administrative OAuth scope.
pub const DEFAULT_ADMIN_SCOPE: &str = "app:admin";
/// Default protected resource path (canonical MCP endpoint).
pub const DEFAULT_RESOURCE_PATH: &str = "/mcp";
/// Default browser login path mounted by the auth router.
pub const DEFAULT_LOGIN_PATH: &str = "/auth/login";
/// Generic data directory used by AuthProfile::default.
pub const DEFAULT_DATA_DIR: &str = ".auth";
/// Generic OAuth client name used for upstream dynamic registration.
pub const DEFAULT_UPSTREAM_CLIENT_NAME: &str = "app";
/// Generic upstream authorization callback path.
pub const DEFAULT_UPSTREAM_CALLBACK_PATH: &str = "/auth/upstream/callback";

/// Product identity and route defaults supplied to the reusable auth engine.
///
/// Applications should normally construct one profile at their composition
/// root and keep product branding out of the shared crate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthProfile {
    pub env_prefix: String,
    pub default_data_dir: PathBuf,
    pub session_cookie_name: String,
    pub scopes_supported: Vec<String>,
    pub resource_path: String,
    pub default_scope: String,
    pub static_token_scopes: Vec<String>,
    pub login_path: String,
    pub enable_dynamic_registration: bool,
    pub disable_static_token_with_oauth: bool,
    pub upstream_client_name: String,
    pub upstream_callback_path: String,
}

impl Default for AuthProfile {
    fn default() -> Self {
        Self {
            env_prefix: DEFAULT_ENV_PREFIX.to_string(),
            default_data_dir: PathBuf::from(DEFAULT_DATA_DIR),
            session_cookie_name: DEFAULT_SESSION_COOKIE_NAME.to_string(),
            scopes_supported: vec![DEFAULT_SCOPE.to_string(), DEFAULT_ADMIN_SCOPE.to_string()],
            resource_path: DEFAULT_RESOURCE_PATH.to_string(),
            default_scope: DEFAULT_SCOPE.to_string(),
            static_token_scopes: vec![DEFAULT_SCOPE.to_string(), DEFAULT_ADMIN_SCOPE.to_string()],
            login_path: DEFAULT_LOGIN_PATH.to_string(),
            enable_dynamic_registration: false,
            disable_static_token_with_oauth: false,
            upstream_client_name: DEFAULT_UPSTREAM_CLIENT_NAME.to_string(),
            upstream_callback_path: DEFAULT_UPSTREAM_CALLBACK_PATH.to_string(),
        }
    }
}
