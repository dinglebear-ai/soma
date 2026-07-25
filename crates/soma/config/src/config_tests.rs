//! Unit tests for src/config.rs

use super::*;
// Tests that set/remove process-global env vars are marked #[serial] so they
// never run concurrently under `cargo test` (nextest already isolates them in
// separate processes).
use serial_test::serial;

struct EnvVarGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let previous = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

#[test]
#[serial]
fn default_data_dir_honors_soma_home_override() {
    let previous = std::env::var_os("SOMA_HOME");
    std::env::set_var("SOMA_HOME", "/tmp/soma-home-override");

    let dir = default_data_dir().expect("SOMA_HOME override should resolve");

    assert_eq!(dir, std::path::PathBuf::from("/tmp/soma-home-override"));
    match previous {
        Some(value) => std::env::set_var("SOMA_HOME", value),
        None => std::env::remove_var("SOMA_HOME"),
    }
}

// ── McpConfig::is_loopback edge cases ─────────────────────────────────────────

fn mcp_with_host(host: &str) -> McpConfig {
    McpConfig {
        host: host.to_string(),
        ..McpConfig::default()
    }
}

#[test]
fn is_loopback_ipv6_bare() {
    // "::1" without brackets — parsed as IpAddr, is_loopback() returns true
    assert!(mcp_with_host("::1").is_loopback(), "::1 should be loopback");
}

#[test]
fn is_loopback_ipv6_bracketed() {
    // "[::1]" bracket-quoted IPv6 — brackets are stripped before parse
    assert!(
        mcp_with_host("[::1]").is_loopback(),
        "[::1] should be loopback"
    );
}

#[test]
fn is_loopback_127_0_0_2() {
    // Any 127.x.x.x address is in the loopback range
    assert!(
        mcp_with_host("127.0.0.2").is_loopback(),
        "127.0.0.2 should be loopback"
    );
}

#[test]
fn is_loopback_0_0_0_0_is_false() {
    // 0.0.0.0 is unspecified, not loopback
    assert!(
        !mcp_with_host("0.0.0.0").is_loopback(),
        "0.0.0.0 should not be loopback"
    );
}

#[test]
fn is_loopback_uppercase_localhost_is_false() {
    // is_loopback only matches the literal "localhost" (case-sensitive)
    assert!(
        !mcp_with_host("LOCALHOST").is_loopback(),
        "LOCALHOST (uppercase) should not be loopback — check is case-sensitive"
    );
}

#[test]
fn is_loopback_subdomain_is_false() {
    // "localhost.example.com" must not be treated as loopback
    assert!(
        !mcp_with_host("localhost.example.com").is_loopback(),
        "localhost.example.com should not be loopback"
    );
}

// ── env_bool helper ───────────────────────────────────────────────────────────
//
// env_bool is private, so we exercise it via a thin wrapper that sets a
// uniquely-named env var, calls the function, and unsets it again.
// Each test uses a distinct key to avoid collisions with parallel test threads.

fn call_env_bool(key: &str, raw: &str) -> anyhow::Result<bool> {
    std::env::set_var(key, raw);
    let mut target = false;
    let result = env_bool(key, &mut target);
    std::env::remove_var(key);
    result.map(|_| target)
}

#[test]
#[serial]
fn env_bool_accepts_1() {
    assert!(call_env_bool("TEST_ENV_BOOL_1", "1").unwrap());
}

#[test]
#[serial]
fn env_bool_accepts_true() {
    assert!(call_env_bool("TEST_ENV_BOOL_TRUE", "true").unwrap());
}

#[test]
#[serial]
fn env_bool_accepts_yes() {
    assert!(call_env_bool("TEST_ENV_BOOL_YES", "yes").unwrap());
}

#[test]
#[serial]
fn env_bool_accepts_0() {
    assert!(!call_env_bool("TEST_ENV_BOOL_0", "0").unwrap());
}

#[test]
#[serial]
fn env_bool_accepts_false() {
    assert!(!call_env_bool("TEST_ENV_BOOL_FALSE", "false").unwrap());
}

#[test]
#[serial]
fn env_bool_accepts_no() {
    assert!(!call_env_bool("TEST_ENV_BOOL_NO", "no").unwrap());
}

#[test]
#[serial]
fn env_bool_rejects_invalid() {
    let result = call_env_bool("TEST_ENV_BOOL_INVALID", "maybe");
    assert!(result.is_err(), "invalid bool string should return Err");
}

// ── env_list helper ───────────────────────────────────────────────────────────

fn call_env_list(key: &str, raw: &str) -> Vec<String> {
    std::env::set_var(key, raw);
    let mut target: Vec<String> = Vec::new();
    env_list(key, &mut target);
    std::env::remove_var(key);
    target
}

#[test]
#[serial]
fn env_list_splits_comma_separated() {
    let result = call_env_list("TEST_ENV_LIST_CSV", "a,b,c");
    assert_eq!(result, vec!["a", "b", "c"]);
}

#[test]
#[serial]
fn env_list_trims_spaces_around_commas() {
    let result = call_env_list("TEST_ENV_LIST_SPACES", "foo , bar , baz");
    assert_eq!(result, vec!["foo", "bar", "baz"]);
}

#[test]
#[serial]
fn env_list_empty_string_leaves_target_unchanged() {
    // An empty env var should not overwrite an existing target
    std::env::set_var("TEST_ENV_LIST_EMPTY", "");
    let mut target = vec!["existing".to_string()];
    env_list("TEST_ENV_LIST_EMPTY", &mut target);
    std::env::remove_var("TEST_ENV_LIST_EMPTY");
    assert_eq!(
        target,
        vec!["existing"],
        "empty env var should not clear target"
    );
}

// ── AuthMode serde parsing ────────────────────────────────────────────────────
//
// AuthMode parsing in Config::load() is an inline match on the env var string,
// not a standalone function. We test the serde Deserialize path instead, which
// exercises the #[serde(rename_all = "lowercase")] attribute.

#[test]
fn auth_mode_deserializes_oauth() {
    let mode: AuthMode = serde_json::from_str("\"oauth\"").expect("oauth should deserialize");
    assert_eq!(mode, AuthMode::OAuth);
}

#[test]
fn auth_mode_deserializes_bearer() {
    let mode: AuthMode = serde_json::from_str("\"bearer\"").expect("bearer should deserialize");
    assert_eq!(mode, AuthMode::Bearer);
}

#[test]
fn auth_mode_rejects_bad_value() {
    let result = serde_json::from_str::<AuthMode>("\"bad\"");
    assert!(
        result.is_err(),
        "unknown auth mode should fail to deserialize"
    );
}

#[test]
fn runtime_mode_auto_uses_api_url_as_compatibility_signal() {
    let local = SomaConfig::default();
    assert_eq!(local.effective_runtime_mode(), EffectiveRuntimeMode::Local);

    let remote = SomaConfig {
        api_url: "https://soma.example.test".into(),
        ..SomaConfig::default()
    };
    assert_eq!(
        remote.effective_runtime_mode(),
        EffectiveRuntimeMode::Remote
    );
}

#[test]
fn runtime_mode_explicit_local_overrides_api_url() {
    let config = SomaConfig {
        api_url: "https://soma.example.test".into(),
        runtime_mode: RuntimeMode::Local,
        ..SomaConfig::default()
    };

    assert_eq!(config.effective_runtime_mode(), EffectiveRuntimeMode::Local);
}

#[test]
fn runtime_mode_deserializes_remote() {
    let mode: RuntimeMode = serde_json::from_str("\"remote\"").expect("remote should deserialize");
    assert_eq!(mode, RuntimeMode::Remote);
}

#[test]
#[serial]
fn config_load_rejects_invalid_runtime_mode_env() {
    let previous = std::env::var_os("SOMA_RUNTIME_MODE");
    std::env::set_var("SOMA_RUNTIME_MODE", "sideways");

    let result = Config::load();

    match previous {
        Some(value) => std::env::set_var("SOMA_RUNTIME_MODE", value),
        None => std::env::remove_var("SOMA_RUNTIME_MODE"),
    }
    assert!(
        result.is_err(),
        "invalid runtime mode should fail config load"
    );
}

// ── AuthConfig env overrides (SOMA_MCP_AUTHELIA_*/SOMA_MCP_GITHUB_*/…) ────────

#[test]
fn auth_config_defaults_leave_builder_owned_fields_unset() {
    // soma-auth owns the default VALUES for these (crates/shared/auth/src/
    // config.rs); soma-config must leave them unset so the synthesized var
    // list omits them and the builder's own defaults apply.
    let auth = AuthConfig::default();
    assert!(auth.sqlite_path.is_none());
    assert!(auth.key_path.is_none());
    assert!(auth.access_token_ttl_secs.is_none());
    assert!(auth.refresh_token_ttl_secs.is_none());
    assert!(auth.auth_code_ttl_secs.is_none());
    assert!(auth.register_rpm.is_none());
    assert!(auth.authorize_rpm.is_none());
    assert!(auth.max_pending_oauth_states.is_none());
    assert!(auth.default_provider.is_none());
    assert!(auth.bootstrap_secret.is_none());
    assert!(auth.token_encryption_key.is_none());
    assert!(auth.google_callback_path.is_none());
    assert!(auth.google_scopes.is_empty());
    assert!(auth.authelia_issuer_url.is_none());
    assert!(auth.authelia_client_id.is_none());
    assert!(auth.authelia_client_secret.is_none());
    assert!(auth.authelia_callback_path.is_none());
    assert!(auth.authelia_scopes.is_empty());
    assert!(auth.github_client_id.is_none());
    assert!(auth.github_client_secret.is_none());
    assert!(auth.github_callback_path.is_none());
    assert!(auth.github_scopes.is_empty());
}

#[test]
#[serial]
fn authelia_and_github_env_overrides_populate_typed_config() {
    let _issuer = EnvVarGuard::set("SOMA_MCP_AUTHELIA_ISSUER_URL", "https://auth.example.com");
    let _a_id = EnvVarGuard::set("SOMA_MCP_AUTHELIA_CLIENT_ID", "authelia-id");
    let _a_secret = EnvVarGuard::set("SOMA_MCP_AUTHELIA_CLIENT_SECRET", "authelia-secret");
    let _a_cb = EnvVarGuard::set("SOMA_MCP_AUTHELIA_CALLBACK_PATH", "/auth/a/callback");
    let _a_scopes = EnvVarGuard::set("SOMA_MCP_AUTHELIA_SCOPES", "openid, profile ,email");
    let _gh_id = EnvVarGuard::set("SOMA_MCP_GITHUB_CLIENT_ID", "gh-id");
    let _gh_secret = EnvVarGuard::set("SOMA_MCP_GITHUB_CLIENT_SECRET", "gh-secret");
    let _gh_cb = EnvVarGuard::set("SOMA_MCP_GITHUB_CALLBACK_PATH", "/auth/gh/callback");
    let _gh_scopes = EnvVarGuard::set("SOMA_MCP_GITHUB_SCOPES", "read:user,user:email");
    let _provider = EnvVarGuard::set("SOMA_MCP_AUTH_DEFAULT_PROVIDER", "authelia");

    let config = Config::load().expect("config should load");
    let auth = &config.mcp.auth;
    assert_eq!(
        auth.authelia_issuer_url.as_deref(),
        Some("https://auth.example.com")
    );
    assert_eq!(auth.authelia_client_id.as_deref(), Some("authelia-id"));
    assert_eq!(
        auth.authelia_client_secret.as_deref(),
        Some("authelia-secret")
    );
    assert_eq!(
        auth.authelia_callback_path.as_deref(),
        Some("/auth/a/callback")
    );
    assert_eq!(auth.authelia_scopes, vec!["openid", "profile", "email"]);
    assert_eq!(auth.github_client_id.as_deref(), Some("gh-id"));
    assert_eq!(auth.github_client_secret.as_deref(), Some("gh-secret"));
    assert_eq!(
        auth.github_callback_path.as_deref(),
        Some("/auth/gh/callback")
    );
    assert_eq!(auth.github_scopes, vec!["read:user", "user:email"]);
    assert_eq!(auth.default_provider.as_deref(), Some("authelia"));
}

#[test]
#[serial]
fn auth_numeric_env_overrides_parse_into_typed_options() {
    let _at = EnvVarGuard::set("SOMA_MCP_AUTH_ACCESS_TOKEN_TTL_SECS", "1234");
    let _rt = EnvVarGuard::set("SOMA_MCP_AUTH_REFRESH_TOKEN_TTL_SECS", "5678");
    let _code = EnvVarGuard::set("SOMA_MCP_AUTH_CODE_TTL_SECS", "90");
    let _reg = EnvVarGuard::set("SOMA_MCP_AUTH_REGISTER_REQUESTS_PER_MINUTE", "7");
    let _az = EnvVarGuard::set("SOMA_MCP_AUTH_AUTHORIZE_REQUESTS_PER_MINUTE", "11");
    let _pending = EnvVarGuard::set("SOMA_MCP_AUTH_MAX_PENDING_OAUTH_STATES", "42");

    let config = Config::load().expect("config should load");
    let auth = &config.mcp.auth;
    assert_eq!(auth.access_token_ttl_secs, Some(1234));
    assert_eq!(auth.refresh_token_ttl_secs, Some(5678));
    assert_eq!(auth.auth_code_ttl_secs, Some(90));
    assert_eq!(auth.register_rpm, Some(7));
    assert_eq!(auth.authorize_rpm, Some(11));
    assert_eq!(auth.max_pending_oauth_states, Some(42));
}

#[test]
#[serial]
fn auth_numeric_env_override_rejects_non_numeric_value() {
    let _at = EnvVarGuard::set("SOMA_MCP_AUTH_ACCESS_TOKEN_TTL_SECS", "soon");

    let error = Config::load().expect_err("non-numeric TTL should be rejected");
    assert!(error
        .to_string()
        .contains("SOMA_MCP_AUTH_ACCESS_TOKEN_TTL_SECS"));
}

#[test]
#[serial]
fn auth_path_secret_and_redirect_env_overrides_populate_typed_config() {
    let _db = EnvVarGuard::set("SOMA_MCP_AUTH_SQLITE_PATH", "/tmp/soma-auth.db");
    let _key = EnvVarGuard::set("SOMA_MCP_AUTH_KEY_PATH", "/tmp/soma-jwt.pem");
    let _secret = EnvVarGuard::set("SOMA_MCP_AUTH_BOOTSTRAP_SECRET", "bootstrap");
    let _redirects = EnvVarGuard::set(
        "SOMA_MCP_AUTH_ALLOWED_REDIRECT_URIS",
        "https://a.example.com/cb, https://b.example.com/cb",
    );
    let _enc = EnvVarGuard::set("SOMA_MCP_TOKEN_ENCRYPTION_KEY", "k".repeat(64));
    let _g_cb = EnvVarGuard::set("SOMA_MCP_GOOGLE_CALLBACK_PATH", "/auth/g/callback");
    let _g_scopes = EnvVarGuard::set("SOMA_MCP_GOOGLE_SCOPES", "openid,email");

    let config = Config::load().expect("config should load");
    let auth = &config.mcp.auth;
    assert_eq!(auth.sqlite_path.as_deref(), Some("/tmp/soma-auth.db"));
    assert_eq!(auth.key_path.as_deref(), Some("/tmp/soma-jwt.pem"));
    assert_eq!(auth.bootstrap_secret.as_deref(), Some("bootstrap"));
    assert_eq!(
        auth.allowed_client_redirect_uris,
        vec!["https://a.example.com/cb", "https://b.example.com/cb"]
    );
    assert_eq!(
        auth.token_encryption_key.as_deref(),
        Some("k".repeat(64).as_str())
    );
    assert_eq!(
        auth.google_callback_path.as_deref(),
        Some("/auth/g/callback")
    );
    assert_eq!(auth.google_scopes, vec!["openid", "email"]);
}

// ── TraceHeaderMode / SOMA_MCP_TRACE_HEADERS ──────────────────────────────────

#[test]
fn trace_headers_default_to_off() {
    assert_eq!(McpConfig::default().trace_headers, TraceHeaderMode::Off);
}

#[test]
#[serial]
fn trace_headers_env_parses_all_three_values() {
    let _env = EnvVarGuard::set("SOMA_MCP_TRACE_HEADERS", "off");

    for (raw, expected) in [
        ("off", TraceHeaderMode::Off),
        ("trusted", TraceHeaderMode::Trusted),
        ("trusted-with-baggage", TraceHeaderMode::TrustedWithBaggage),
    ] {
        std::env::set_var("SOMA_MCP_TRACE_HEADERS", raw);
        let config = Config::load().expect("config should load");
        assert_eq!(
            config.mcp.trace_headers, expected,
            "SOMA_MCP_TRACE_HEADERS={raw:?} should parse to {expected:?}"
        );
    }
}

#[test]
#[serial]
fn trace_headers_env_rejects_invalid_value() {
    let _env = EnvVarGuard::set("SOMA_MCP_TRACE_HEADERS", "bogus");

    let error = Config::load().expect_err("invalid SOMA_MCP_TRACE_HEADERS should be rejected");
    assert!(error.to_string().contains("SOMA_MCP_TRACE_HEADERS"));
}

#[test]
fn trace_header_mode_serde_uses_kebab_case() {
    assert_eq!(
        serde_json::to_value(TraceHeaderMode::TrustedWithBaggage).unwrap(),
        serde_json::json!("trusted-with-baggage")
    );
    assert_eq!(
        serde_json::from_value::<TraceHeaderMode>(serde_json::json!("trusted")).unwrap(),
        TraceHeaderMode::Trusted
    );
}

#[test]
fn trace_header_mode_display_uses_accepted_config_values() {
    for (mode, expected) in [
        (TraceHeaderMode::Off, "off"),
        (TraceHeaderMode::Trusted, "trusted"),
        (TraceHeaderMode::TrustedWithBaggage, "trusted-with-baggage"),
    ] {
        assert_eq!(mode.as_str(), expected);
        assert_eq!(mode.to_string(), expected);
    }
}

#[test]
fn trace_headers_toml_file_config_parses_all_three_values() {
    // Exercises the same `toml::from_str::<Config>` path `Config::load()` uses
    // for `config.toml`, without needing filesystem/cwd scaffolding — proves
    // file config (not just env) parses `mcp.trace_headers`.
    for (raw, expected) in [
        ("off", TraceHeaderMode::Off),
        ("trusted", TraceHeaderMode::Trusted),
        ("trusted-with-baggage", TraceHeaderMode::TrustedWithBaggage),
    ] {
        let toml_str = format!("[mcp]\ntrace_headers = \"{raw}\"\n");
        let config: Config = toml::from_str(&toml_str).expect("toml should parse");
        assert_eq!(config.mcp.trace_headers, expected, "raw TOML value {raw:?}");
    }
}
