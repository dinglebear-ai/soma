//! Configuration structs for the Soma MCP server.
//!
//! Values are loaded in priority order:
//!   1. `config.toml` (checked in, defaults only — no secrets)
//!   2. Environment variables (`SOMA_*`, `SOMA_MCP_*`)
//!
//! **Customize**: rename `SomaConfig` to match your service. Adjust env prefixes
//! throughout. Add any domain-specific config fields you need.

use serde::{Deserialize, Serialize};

/// CUSTOMIZE: Replace with your service name (e.g. ".unraid", ".gotify").
const SERVICE_HOME_DIRNAME: &str = ".soma";

/// Top-level config (maps to `config.toml` sections).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub mcp: McpConfig,
    pub soma: SomaConfig,
}

/// Config for the Soma runtime or deployed platform API.
///
/// For application/platform servers, the local CLI + stdio MCP adapter uses
/// `api_url` as the deployed `soma serve` API base URL. For upstream-client
/// servers, replace this with config for the actual upstream service.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SomaConfig {
    /// Base URL of the deployed platform API or upstream service (SOMA_API_URL).
    /// Example: `https://example.example.com/`
    pub api_url: String,
    /// API key or bearer token (SOMA_API_KEY).
    pub api_key: String,
    /// Runtime adapter mode (SOMA_RUNTIME_MODE).
    pub runtime_mode: RuntimeMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeMode {
    /// Preserve compatibility: remote when SOMA_API_URL is set, otherwise local.
    #[default]
    Auto,
    /// Execute business actions in this process.
    Local,
    /// Treat this binary as a client adapter for a running HTTP server.
    Remote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveRuntimeMode {
    Local,
    Remote,
}

impl SomaConfig {
    pub fn effective_runtime_mode(&self) -> EffectiveRuntimeMode {
        match self.runtime_mode {
            RuntimeMode::Auto if self.api_url.trim().is_empty() => EffectiveRuntimeMode::Local,
            RuntimeMode::Auto => EffectiveRuntimeMode::Remote,
            RuntimeMode::Local => EffectiveRuntimeMode::Local,
            RuntimeMode::Remote => EffectiveRuntimeMode::Remote,
        }
    }

    pub fn is_remote_adapter(&self) -> bool {
        self.effective_runtime_mode() == EffectiveRuntimeMode::Remote
    }
}

/// MCP HTTP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct McpConfig {
    /// Bind host (SOMA_MCP_HOST). Default: `127.0.0.1` (loopback).
    /// Set to `0.0.0.0` to listen on all interfaces — requires auth configured.
    #[serde(default = "default_mcp_host")]
    pub host: String,
    /// Bind port (SOMA_MCP_PORT). Default: `40060`.
    #[serde(default = "default_mcp_port")]
    pub port: u16,
    /// MCP server name advertised to clients (SOMA_MCP_SERVER_NAME).
    #[serde(default = "default_server_name")]
    pub server_name: String,
    /// Disable auth entirely — only safe when bound to loopback (SOMA_MCP_NO_AUTH).
    pub no_auth: bool,
    /// Allow unauthenticated access on non-loopback when behind a trusted reverse proxy
    /// that enforces its own auth (SOMA_NOAUTH). Loaded here so it participates in
    /// typed config rather than being a raw env read at call sites.
    pub trusted_gateway: bool,
    /// Advertise official MCP conformance reference fixtures.
    ///
    /// This is a test harness switch, not part of Soma's production
    /// surface. Keep it false for real derived servers.
    pub conformance_fixtures: bool,
    /// Static bearer token for simple auth (SOMA_MCP_TOKEN).
    pub api_token: Option<String>,
    /// Grant the static bearer token `soma:write` in addition to the
    /// default `soma:read` (SOMA_MCP_STATIC_TOKEN_WRITE). Off by default so
    /// a leaked static token cannot perform write actions unless the
    /// operator explicitly opted in (pattern ported from cortex's
    /// `static_token_is_admin`).
    pub static_token_write: bool,
    /// Additional allowed Host header values (comma-separated in env).
    pub allowed_hosts: Vec<String>,
    /// Additional allowed CORS origins (comma-separated in env).
    pub allowed_origins: Vec<String>,
    /// Trusted HTTP trace-header extraction mode (SOMA_MCP_TRACE_HEADERS).
    /// Only meaningful when the resolved auth policy is a real trust boundary
    /// (loopback bind or a trusted gateway) — see
    /// `soma_runtime::server::resolve_auth_policy_kind`.
    pub trace_headers: TraceHeaderMode,
    /// OAuth sub-config (nested under `[mcp.auth]` in config.toml).
    pub auth: AuthConfig,
}

impl McpConfig {
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Return true if the configured bind host resolves to a loopback address.
    ///
    /// Uses `IpAddr::is_loopback()` for numeric addresses. Accepts "localhost"
    /// as a canonical loopback hostname. Any other hostname or parse failure is
    /// treated as non-loopback — callers must not assume safety in that case.
    pub fn is_loopback(&self) -> bool {
        let host = &self.host;
        // Match "localhost" literal and numeric loopback addresses.
        // Strip bracket notation ([::1]) before parsing so IPv6 loopback works.
        host == "localhost"
            || host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<std::net::IpAddr>()
                .map(|ip| ip.is_loopback())
                .unwrap_or(false)
    }
}

/// OAuth / JWT auth sub-config.
///
/// This struct types every env var `soma_auth::AuthConfigBuilder` consumes
/// (`SOMA_MCP_*`). Fields left unset (`None` / empty) are deliberately NOT
/// given soma-side defaults: `soma_integrations::auth` synthesizes a var list
/// from set fields only, so the auth crate's own defaults (see
/// `crates/shared/auth/src/config.rs`) apply exactly as they would have when
/// the builder read process env directly.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub mode: AuthMode,
    /// Public base URL for OAuth metadata (SOMA_MCP_PUBLIC_URL).
    pub public_url: Option<String>,
    /// Google OAuth client ID (SOMA_MCP_GOOGLE_CLIENT_ID).
    pub google_client_id: Option<String>,
    /// Google OAuth client secret (SOMA_MCP_GOOGLE_CLIENT_SECRET).
    pub google_client_secret: Option<String>,
    /// Google OAuth callback path override (SOMA_MCP_GOOGLE_CALLBACK_PATH).
    pub google_callback_path: Option<String>,
    /// Google OAuth scopes override (SOMA_MCP_GOOGLE_SCOPES, comma-separated in env).
    pub google_scopes: Vec<String>,
    /// Authelia OIDC issuer URL (SOMA_MCP_AUTHELIA_ISSUER_URL, must be https).
    pub authelia_issuer_url: Option<String>,
    /// Authelia OIDC client ID (SOMA_MCP_AUTHELIA_CLIENT_ID).
    pub authelia_client_id: Option<String>,
    /// Authelia OIDC client secret (SOMA_MCP_AUTHELIA_CLIENT_SECRET).
    pub authelia_client_secret: Option<String>,
    /// Authelia callback path override (SOMA_MCP_AUTHELIA_CALLBACK_PATH).
    pub authelia_callback_path: Option<String>,
    /// Authelia scopes override (SOMA_MCP_AUTHELIA_SCOPES, comma-separated in env).
    pub authelia_scopes: Vec<String>,
    /// GitHub OAuth App client ID (SOMA_MCP_GITHUB_CLIENT_ID).
    pub github_client_id: Option<String>,
    /// GitHub OAuth App client secret (SOMA_MCP_GITHUB_CLIENT_SECRET).
    pub github_client_secret: Option<String>,
    /// GitHub callback path override (SOMA_MCP_GITHUB_CALLBACK_PATH).
    pub github_callback_path: Option<String>,
    /// GitHub scopes override (SOMA_MCP_GITHUB_SCOPES, comma-separated in env;
    /// must include `user:email`).
    pub github_scopes: Vec<String>,
    /// Default OAuth provider (SOMA_MCP_AUTH_DEFAULT_PROVIDER). Unset =
    /// automatic priority: Google, Authelia, GitHub.
    pub default_provider: Option<String>,
    /// OAuth admin email (SOMA_MCP_AUTH_ADMIN_EMAIL).
    pub admin_email: String,
    pub allowed_emails: Vec<String>,
    /// Native-flow bootstrap secret (SOMA_MCP_AUTH_BOOTSTRAP_SECRET).
    pub bootstrap_secret: Option<String>,
    /// Auth SQLite DB path (SOMA_MCP_AUTH_SQLITE_PATH).
    pub sqlite_path: Option<String>,
    /// Ed25519 JWT signing key path (SOMA_MCP_AUTH_KEY_PATH).
    pub key_path: Option<String>,
    /// Access-token TTL in seconds (SOMA_MCP_AUTH_ACCESS_TOKEN_TTL_SECS).
    pub access_token_ttl_secs: Option<u64>,
    /// Refresh-token TTL in seconds (SOMA_MCP_AUTH_REFRESH_TOKEN_TTL_SECS).
    pub refresh_token_ttl_secs: Option<u64>,
    /// Auth-code TTL in seconds (SOMA_MCP_AUTH_CODE_TTL_SECS).
    pub auth_code_ttl_secs: Option<u64>,
    /// `/register` rate limit (SOMA_MCP_AUTH_REGISTER_REQUESTS_PER_MINUTE).
    pub register_rpm: Option<u32>,
    /// `/authorize` rate limit (SOMA_MCP_AUTH_AUTHORIZE_REQUESTS_PER_MINUTE).
    pub authorize_rpm: Option<u32>,
    /// Pending OAuth state cap (SOMA_MCP_AUTH_MAX_PENDING_OAUTH_STATES).
    pub max_pending_oauth_states: Option<usize>,
    /// Allowed dynamic-client redirect URIs (SOMA_MCP_AUTH_ALLOWED_REDIRECT_URIS,
    /// comma-separated in env).
    pub allowed_client_redirect_uris: Vec<String>,
    /// At-rest refresh-token encryption key (SOMA_MCP_TOKEN_ENCRYPTION_KEY,
    /// 64 hex digits or 43 base64url chars — validated by soma-auth).
    pub token_encryption_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    #[default]
    Bearer,
    OAuth,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TraceHeaderMode {
    /// No HTTP trace-header extraction. Default — safe for every deployment.
    #[default]
    Off,
    /// Extract `traceparent`/`tracestate` from inbound HTTP headers after auth.
    /// Baggage is never extracted in this mode.
    Trusted,
    /// Like `Trusted`, but also extracts validated `baggage`. Baggage can carry
    /// sensitive user/session/application data — enable deliberately.
    TrustedWithBaggage,
}

impl TraceHeaderMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Trusted => "trusted",
            Self::TrustedWithBaggage => "trusted-with-baggage",
        }
    }
}

impl std::fmt::Display for TraceHeaderMode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

// ── defaults ──────────────────────────────────────────────────────────────────

fn default_mcp_host() -> String {
    // Default to loopback for safety. Operators who need external access must
    // explicitly set SOMA_MCP_HOST=0.0.0.0 (and configure auth).
    "127.0.0.1".into()
}
fn default_mcp_port() -> u16 {
    40060
}
fn default_server_name() -> String {
    "soma".into()
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            host: default_mcp_host(),
            port: default_mcp_port(),
            server_name: default_server_name(),
            no_auth: false,
            trusted_gateway: false,
            conformance_fixtures: false,
            api_token: None,
            static_token_write: false,
            allowed_hosts: Vec::new(),
            allowed_origins: Vec::new(),
            trace_headers: TraceHeaderMode::default(),
            auth: AuthConfig::default(),
        }
    }
}

// ── Appdata directory ─────────────────────────────────────────────────────────

/// Return the default local data directory for this service.
///
/// Pattern §25 + §28: The same `.env` and `config.toml` in `~/.<service>/`
/// work for both Docker and bare-metal deployment without modification.
///
/// | Environment   | Path                                |
/// |---------------|-------------------------------------|
/// | Container     | `/data` (bind-mounted from host)     |
/// | Bare-metal    | `~/.soma` (user home dir)        |
///
/// The name should match the docker-compose.yml volume mount source.
pub fn default_data_dir() -> anyhow::Result<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("SOMA_HOME") {
        return Ok(std::path::PathBuf::from(path));
    }

    // Running inside a Docker container — /data is always the mount point.
    // Detection uses /.dockerenv (created by the Docker runtime) or an explicit
    // RUNNING_IN_CONTAINER env var (useful for testing or systemd-nspawn).
    if std::path::Path::new("/.dockerenv").exists()
        || std::env::var("RUNNING_IN_CONTAINER").is_ok()
        || std::env::var("container").is_ok()
    {
        return Ok(std::path::PathBuf::from("/data"));
    }

    // Bare-metal or local dev — use ~/.<service>/
    let home = dirs::home_dir().ok_or_else(|| {
        anyhow::anyhow!("cannot determine home directory — set HOME or RUNNING_IN_CONTAINER=1")
    })?;
    Ok(home.join(SERVICE_HOME_DIRNAME))
}

/// Load `<appdata>/.env` (`~/.<service>/.env` on bare metal, `/data/.env` in a
/// container) into the process environment if present.
///
/// Best-effort: a missing file is ignored, and existing env vars are NOT
/// overridden — values injected by docker-compose/systemd or the plugin hook's
/// `CLAUDE_PLUGIN_OPTION_*` mapping still take precedence. This lets the binary
/// find its credentials directly from `~/.<service>/.env` without relying on a
/// process manager to inject them. Call once at startup before `Config::load`.
pub fn load_dotenv() {
    let Ok(dir) = default_data_dir() else {
        return;
    };
    let env_path = dir.join(".env");
    // Reject symlinks under the appdata dir — it holds secrets and we do not want
    // a planted symlink redirecting us to attacker-controlled env (mirrors axon).
    // Bare `dotenvy::from_path` would follow the symlink via `File::open`.
    match std::fs::symlink_metadata(&env_path) {
        Ok(md) if md.file_type().is_symlink() => {
            eprintln!(
                "error: refusing to load symlinked .env at {} (potential symlink attack)",
                env_path.display()
            );
            std::process::exit(1);
        }
        Ok(_) => {
            let _ = dotenvy::from_path(&env_path);
        }
        // Missing or inaccessible — best effort; fall back to process env / config.toml.
        Err(_) => {}
    }
}

// ── Config loading ────────────────────────────────────────────────────────────

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let mut config = Config::default();

        // Search for config.toml in priority order (§25: appdata convention):
        //   1. ~/<SERVICE_HOME_DIRNAME>/config.toml  — user's persistent config (primary)
        //   2. ./config.toml                         — local dev / Docker mount fallback
        let candidate_paths = {
            let mut paths = vec![];
            if let Some(home) = std::env::var_os("HOME") {
                paths.push(
                    std::path::PathBuf::from(home)
                        .join(SERVICE_HOME_DIRNAME)
                        .join("config.toml"),
                );
            }
            paths.push(std::path::PathBuf::from("config.toml"));
            paths
        };

        for path in &candidate_paths {
            match std::fs::read_to_string(path) {
                Ok(contents) => {
                    config = toml::from_str(&contents)
                        .map_err(|e| anyhow::anyhow!("Failed to parse {}: {e}", path.display()))?;
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(anyhow::anyhow!("Failed to read {}: {e}", path.display())),
            }
        }

        // Env overrides — SOMA_MCP_* for server config, SOMA_API_* for upstream
        env_str("SOMA_MCP_HOST", &mut config.mcp.host);
        env_parse("SOMA_MCP_PORT", &mut config.mcp.port)?;
        env_str("SOMA_MCP_SERVER_NAME", &mut config.mcp.server_name);
        env_bool("SOMA_MCP_NO_AUTH", &mut config.mcp.no_auth)?;
        env_bool("SOMA_NOAUTH", &mut config.mcp.trusted_gateway)?;
        env_bool(
            "SOMA_MCP_CONFORMANCE_FIXTURES",
            &mut config.mcp.conformance_fixtures,
        )?;
        env_opt_str("SOMA_MCP_TOKEN", &mut config.mcp.api_token);
        env_bool(
            "SOMA_MCP_STATIC_TOKEN_WRITE",
            &mut config.mcp.static_token_write,
        )?;
        env_list("SOMA_MCP_ALLOWED_HOSTS", &mut config.mcp.allowed_hosts);
        env_list("SOMA_MCP_ALLOWED_ORIGINS", &mut config.mcp.allowed_origins);
        env_opt_str("SOMA_MCP_PUBLIC_URL", &mut config.mcp.auth.public_url);
        env_str(
            "SOMA_MCP_AUTH_ADMIN_EMAIL",
            &mut config.mcp.auth.admin_email,
        );
        env_opt_str(
            "SOMA_MCP_GOOGLE_CLIENT_ID",
            &mut config.mcp.auth.google_client_id,
        );
        env_opt_str(
            "SOMA_MCP_GOOGLE_CLIENT_SECRET",
            &mut config.mcp.auth.google_client_secret,
        );
        env_opt_str(
            "SOMA_MCP_GOOGLE_CALLBACK_PATH",
            &mut config.mcp.auth.google_callback_path,
        );
        env_list("SOMA_MCP_GOOGLE_SCOPES", &mut config.mcp.auth.google_scopes);
        env_opt_str(
            "SOMA_MCP_AUTHELIA_ISSUER_URL",
            &mut config.mcp.auth.authelia_issuer_url,
        );
        env_opt_str(
            "SOMA_MCP_AUTHELIA_CLIENT_ID",
            &mut config.mcp.auth.authelia_client_id,
        );
        env_opt_str(
            "SOMA_MCP_AUTHELIA_CLIENT_SECRET",
            &mut config.mcp.auth.authelia_client_secret,
        );
        env_opt_str(
            "SOMA_MCP_AUTHELIA_CALLBACK_PATH",
            &mut config.mcp.auth.authelia_callback_path,
        );
        env_list(
            "SOMA_MCP_AUTHELIA_SCOPES",
            &mut config.mcp.auth.authelia_scopes,
        );
        env_opt_str(
            "SOMA_MCP_GITHUB_CLIENT_ID",
            &mut config.mcp.auth.github_client_id,
        );
        env_opt_str(
            "SOMA_MCP_GITHUB_CLIENT_SECRET",
            &mut config.mcp.auth.github_client_secret,
        );
        env_opt_str(
            "SOMA_MCP_GITHUB_CALLBACK_PATH",
            &mut config.mcp.auth.github_callback_path,
        );
        env_list("SOMA_MCP_GITHUB_SCOPES", &mut config.mcp.auth.github_scopes);
        env_opt_str(
            "SOMA_MCP_AUTH_DEFAULT_PROVIDER",
            &mut config.mcp.auth.default_provider,
        );
        env_opt_str(
            "SOMA_MCP_AUTH_BOOTSTRAP_SECRET",
            &mut config.mcp.auth.bootstrap_secret,
        );
        env_opt_str(
            "SOMA_MCP_AUTH_SQLITE_PATH",
            &mut config.mcp.auth.sqlite_path,
        );
        env_opt_str("SOMA_MCP_AUTH_KEY_PATH", &mut config.mcp.auth.key_path);
        env_opt_parse(
            "SOMA_MCP_AUTH_ACCESS_TOKEN_TTL_SECS",
            &mut config.mcp.auth.access_token_ttl_secs,
        )?;
        env_opt_parse(
            "SOMA_MCP_AUTH_REFRESH_TOKEN_TTL_SECS",
            &mut config.mcp.auth.refresh_token_ttl_secs,
        )?;
        env_opt_parse(
            "SOMA_MCP_AUTH_CODE_TTL_SECS",
            &mut config.mcp.auth.auth_code_ttl_secs,
        )?;
        env_opt_parse(
            "SOMA_MCP_AUTH_REGISTER_REQUESTS_PER_MINUTE",
            &mut config.mcp.auth.register_rpm,
        )?;
        env_opt_parse(
            "SOMA_MCP_AUTH_AUTHORIZE_REQUESTS_PER_MINUTE",
            &mut config.mcp.auth.authorize_rpm,
        )?;
        env_opt_parse(
            "SOMA_MCP_AUTH_MAX_PENDING_OAUTH_STATES",
            &mut config.mcp.auth.max_pending_oauth_states,
        )?;
        env_list(
            "SOMA_MCP_AUTH_ALLOWED_REDIRECT_URIS",
            &mut config.mcp.auth.allowed_client_redirect_uris,
        );
        env_opt_str(
            "SOMA_MCP_TOKEN_ENCRYPTION_KEY",
            &mut config.mcp.auth.token_encryption_key,
        );
        if let Ok(v) = std::env::var("SOMA_MCP_AUTH_MODE") {
            if !v.is_empty() {
                config.mcp.auth.mode = match v.to_lowercase().as_str() {
                    "oauth" => AuthMode::OAuth,
                    "bearer" => AuthMode::Bearer,
                    other => {
                        return Err(anyhow::anyhow!(
                            "invalid SOMA_MCP_AUTH_MODE {:?}: must be \"bearer\" or \"oauth\"",
                            other
                        ));
                    }
                };
            }
        }
        if let Ok(v) = std::env::var("SOMA_MCP_TRACE_HEADERS") {
            if !v.is_empty() {
                config.mcp.trace_headers = match v.to_lowercase().as_str() {
                    "off" => TraceHeaderMode::Off,
                    "trusted" => TraceHeaderMode::Trusted,
                    "trusted-with-baggage" => TraceHeaderMode::TrustedWithBaggage,
                    other => {
                        return Err(anyhow::anyhow!(
                            "invalid SOMA_MCP_TRACE_HEADERS {:?}: must be \"off\", \"trusted\", \
                             or \"trusted-with-baggage\"",
                            other
                        ));
                    }
                };
            }
        }

        // Upstream service config
        env_str("SOMA_API_URL", &mut config.soma.api_url);
        env_str("SOMA_API_KEY", &mut config.soma.api_key);
        if let Ok(v) = std::env::var("SOMA_RUNTIME_MODE") {
            if !v.is_empty() {
                config.soma.runtime_mode = match v.to_lowercase().as_str() {
                    "auto" => RuntimeMode::Auto,
                    "local" => RuntimeMode::Local,
                    "remote" | "api" => RuntimeMode::Remote,
                    other => {
                        return Err(anyhow::anyhow!(
                            "invalid SOMA_RUNTIME_MODE {:?}: must be \"auto\", \"local\", or \"remote\"",
                            other
                        ));
                    }
                };
            }
        }

        Ok(config)
    }
}

// ── env helpers ───────────────────────────────────────────────────────────────

fn env_str(key: &str, target: &mut String) {
    if let Ok(v) = std::env::var(key) {
        if !v.is_empty() {
            *target = v;
        }
    }
}

fn env_opt_str(key: &str, target: &mut Option<String>) {
    if let Ok(v) = std::env::var(key) {
        if !v.is_empty() {
            *target = Some(v);
        }
    }
}

fn env_parse<T: std::str::FromStr>(key: &str, target: &mut T) -> anyhow::Result<()> {
    if let Ok(v) = std::env::var(key) {
        if !v.is_empty() {
            *target = v
                .parse()
                .map_err(|_| anyhow::anyhow!("{key}: invalid value {v:?}"))?;
        }
    }
    Ok(())
}

fn env_opt_parse<T: std::str::FromStr>(key: &str, target: &mut Option<T>) -> anyhow::Result<()> {
    if let Ok(v) = std::env::var(key) {
        if !v.is_empty() {
            *target = Some(
                v.parse()
                    .map_err(|_| anyhow::anyhow!("{key}: invalid value {v:?}"))?,
            );
        }
    }
    Ok(())
}

fn env_bool(key: &str, target: &mut bool) -> anyhow::Result<()> {
    if let Ok(v) = std::env::var(key) {
        match v.to_lowercase().as_str() {
            "1" | "true" | "yes" => *target = true,
            "0" | "false" | "no" => *target = false,
            other => anyhow::bail!("{key}: expected bool, got {other:?}"),
        }
    }
    Ok(())
}

fn env_list(key: &str, target: &mut Vec<String>) {
    if let Ok(v) = std::env::var(key) {
        let items: Vec<String> = v
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if !items.is_empty() {
            *target = items;
        }
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
