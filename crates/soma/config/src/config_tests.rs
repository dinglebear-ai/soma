//! Unit tests for src/config.rs

use super::*;
// Tests that set/remove process-global env vars are marked #[serial] so they
// never run concurrently under `cargo test` (nextest already isolates them in
// separate processes).
use serial_test::serial;

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
