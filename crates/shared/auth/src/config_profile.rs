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
            default_data_dir: resolve_default_data_dir(),
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

/// Resolve the generic fallback data directory used by [`AuthProfile::default`]
/// (and therefore `AuthConfigBuilder::new()`) for the SQLite token store and
/// the Ed25519 JWT signing key.
///
/// A bare relative path (the literal [`DEFAULT_DATA_DIR`], e.g. `./.auth`) is
/// cwd-dependent: the very same long-running process resolves to a different
/// directory depending on where it happened to be launched from, which can
/// silently split or lose the token store and signing key across restarts.
/// This restores directory resolution to the OS/user level, matching the
/// behavior of this crate before its data directory became configurable via
/// [`AuthProfile`], while staying generic and dependency-light — no homelab
/// or product-specific path is hard-coded here:
///
/// 1. The platform data directory (`dirs::data_dir()` — `$XDG_DATA_HOME` or
///    `~/.local/share` on Linux, `~/Library/Application Support` on macOS,
///    `%APPDATA%` on Windows), joined with a product-neutral subdirectory.
/// 2. The user's home directory (`dirs::home_dir()`) joined with
///    [`DEFAULT_DATA_DIR`], when the platform data directory is unknown but a
///    home directory is.
/// 3. The bare relative [`DEFAULT_DATA_DIR`], only as an absolute last resort
///    (e.g. a minimal container with neither set).
///
/// Consumers embedding this crate should still normally set an explicit,
/// product-owned directory via [`AuthProfile::default_data_dir`] rather than
/// relying on this fallback.
fn resolve_default_data_dir() -> PathBuf {
    if let Some(data_dir) = dirs::data_dir() {
        return data_dir.join(DEFAULT_DATA_DIR.trim_start_matches('.'));
    }
    if let Some(home) = dirs::home_dir() {
        return home.join(DEFAULT_DATA_DIR);
    }
    PathBuf::from(DEFAULT_DATA_DIR)
}

#[cfg(test)]
mod tests {
    use super::{AuthProfile, DEFAULT_DATA_DIR, PathBuf, resolve_default_data_dir};

    /// Pins the resolution order documented on [`resolve_default_data_dir`]:
    /// platform data dir, then home dir, then the bare relative path — so a
    /// future edit can't silently reorder or drop a tier.
    #[test]
    fn resolve_default_data_dir_follows_documented_precedence() {
        let expected = dirs::data_dir()
            .map(|dir| dir.join(DEFAULT_DATA_DIR.trim_start_matches('.')))
            .or_else(|| dirs::home_dir().map(|home| home.join(DEFAULT_DATA_DIR)))
            .unwrap_or_else(|| PathBuf::from(DEFAULT_DATA_DIR));

        assert_eq!(resolve_default_data_dir(), expected);
    }

    /// Regression guard for the cwd-dependent bug: whenever the environment
    /// exposes a platform data dir or a home dir (true for every real
    /// dev/CI machine), `AuthProfile::default()` must NOT resolve the token
    /// store and signing key to the bare relative `.auth`.
    #[test]
    fn default_profile_does_not_use_bare_relative_dir_when_home_is_known() {
        if dirs::data_dir().is_none() && dirs::home_dir().is_none() {
            // No environment information available at all (e.g. a stripped
            // container with neither XDG nor HOME/USERPROFILE set) — the
            // relative fallback is correct here, not a regression.
            return;
        }

        let profile = AuthProfile::default();

        assert_ne!(
            profile.default_data_dir,
            PathBuf::from(DEFAULT_DATA_DIR),
            "default_data_dir must not be the bare relative path when a \
             platform data dir or home dir is available: {:?}",
            profile.default_data_dir
        );
        assert!(
            profile.default_data_dir.is_absolute(),
            "default_data_dir should resolve to an absolute path, got {:?}",
            profile.default_data_dir
        );
    }
}
