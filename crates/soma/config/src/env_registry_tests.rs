use super::*;

#[test]
fn registry_contains_core_runtime_keys() {
    let keys: Vec<&str> = all_specs().iter().map(|spec| spec.key).collect();
    for expected in [
        "SOMA_API_URL",
        "SOMA_API_KEY",
        "SOMA_MCP_TOKEN",
        "SOMA_MCP_AUTHELIA_CLIENT_ID",
        "SOMA_MCP_GITHUB_CLIENT_ID",
        "SOMA_MCP_AUTH_DEFAULT_PROVIDER",
        "SOMA_MCP_HOST",
        "SOMA_MCP_PORT",
    ] {
        assert!(keys.contains(&expected), "missing {expected}");
    }
}

#[test]
fn secret_keys_are_marked_secret() {
    assert!(spec_for("SOMA_API_KEY").unwrap().secret);
    assert!(spec_for("SOMA_MCP_TOKEN").unwrap().secret);
    assert!(spec_for("SOMA_MCP_AUTHELIA_CLIENT_SECRET").unwrap().secret);
    assert!(spec_for("SOMA_MCP_GITHUB_CLIENT_SECRET").unwrap().secret);
    assert!(!spec_for("SOMA_MCP_AUTHELIA_ISSUER_URL").unwrap().secret);
    assert!(!spec_for("SOMA_API_URL").unwrap().secret);
}

#[test]
fn api_env_destinations_match_soma_config_section() {
    assert_eq!(
        spec_for("SOMA_API_URL").unwrap().toml_destination,
        Some("soma.api_url")
    );
    assert_eq!(
        spec_for("SOMA_API_KEY").unwrap().toml_destination,
        Some("soma.api_key")
    );
}

#[test]
fn plugin_option_mapping_is_derived_from_specs() {
    let mappings: Vec<_> = plugin_option_mappings().collect();
    assert!(mappings.contains(&("CLAUDE_PLUGIN_OPTION_SOMA_API_URL", "SOMA_API_URL")));
    assert!(mappings.contains(&("CLAUDE_PLUGIN_OPTION_API_TOKEN", "SOMA_MCP_TOKEN")));
    assert!(mappings.contains(&(
        "CLAUDE_PLUGIN_OPTION_GOOGLE_CLIENT_SECRET",
        "SOMA_MCP_GOOGLE_CLIENT_SECRET"
    )));
}

#[test]
fn trace_headers_env_is_registered_and_maps_to_mcp_config() {
    let spec =
        spec_for("SOMA_MCP_TRACE_HEADERS").expect("SOMA_MCP_TRACE_HEADERS should be registered");
    assert_eq!(spec.toml_destination, Some("mcp.trace_headers"));
    assert!(!spec.secret, "trace-header mode is not a secret");
    assert_eq!(spec.classification, EnvClassification::KeepEnv);
}

#[test]
fn newly_typed_auth_keys_are_registered_with_mcp_auth_destinations() {
    // Every auth env var that now flows through the typed `[mcp.auth]`
    // config section (rmcp-template-cm25) must carry a matching
    // `toml_destination` so env docs and plugin mapping stay in sync.
    for (key, dest) in [
        (
            "SOMA_MCP_GOOGLE_CALLBACK_PATH",
            "mcp.auth.google_callback_path",
        ),
        ("SOMA_MCP_GOOGLE_SCOPES", "mcp.auth.google_scopes"),
        (
            "SOMA_MCP_AUTHELIA_ISSUER_URL",
            "mcp.auth.authelia_issuer_url",
        ),
        ("SOMA_MCP_AUTHELIA_CLIENT_ID", "mcp.auth.authelia_client_id"),
        (
            "SOMA_MCP_AUTHELIA_CLIENT_SECRET",
            "mcp.auth.authelia_client_secret",
        ),
        (
            "SOMA_MCP_AUTHELIA_CALLBACK_PATH",
            "mcp.auth.authelia_callback_path",
        ),
        ("SOMA_MCP_AUTHELIA_SCOPES", "mcp.auth.authelia_scopes"),
        ("SOMA_MCP_GITHUB_CLIENT_ID", "mcp.auth.github_client_id"),
        (
            "SOMA_MCP_GITHUB_CLIENT_SECRET",
            "mcp.auth.github_client_secret",
        ),
        (
            "SOMA_MCP_GITHUB_CALLBACK_PATH",
            "mcp.auth.github_callback_path",
        ),
        ("SOMA_MCP_GITHUB_SCOPES", "mcp.auth.github_scopes"),
        (
            "SOMA_MCP_AUTH_DEFAULT_PROVIDER",
            "mcp.auth.default_provider",
        ),
        (
            "SOMA_MCP_AUTH_BOOTSTRAP_SECRET",
            "mcp.auth.bootstrap_secret",
        ),
        ("SOMA_MCP_AUTH_SQLITE_PATH", "mcp.auth.sqlite_path"),
        ("SOMA_MCP_AUTH_KEY_PATH", "mcp.auth.key_path"),
        (
            "SOMA_MCP_AUTH_ACCESS_TOKEN_TTL_SECS",
            "mcp.auth.access_token_ttl_secs",
        ),
        (
            "SOMA_MCP_AUTH_REFRESH_TOKEN_TTL_SECS",
            "mcp.auth.refresh_token_ttl_secs",
        ),
        ("SOMA_MCP_AUTH_CODE_TTL_SECS", "mcp.auth.auth_code_ttl_secs"),
        (
            "SOMA_MCP_AUTH_REGISTER_REQUESTS_PER_MINUTE",
            "mcp.auth.register_rpm",
        ),
        (
            "SOMA_MCP_AUTH_AUTHORIZE_REQUESTS_PER_MINUTE",
            "mcp.auth.authorize_rpm",
        ),
        (
            "SOMA_MCP_AUTH_MAX_PENDING_OAUTH_STATES",
            "mcp.auth.max_pending_oauth_states",
        ),
        (
            "SOMA_MCP_AUTH_ALLOWED_REDIRECT_URIS",
            "mcp.auth.allowed_client_redirect_uris",
        ),
        (
            "SOMA_MCP_TOKEN_ENCRYPTION_KEY",
            "mcp.auth.token_encryption_key",
        ),
        ("SOMA_MCP_STATIC_TOKEN_WRITE", "mcp.static_token_write"),
    ] {
        let spec = spec_for(key).unwrap_or_else(|| panic!("{key} should be registered"));
        assert_eq!(
            spec.toml_destination,
            Some(dest),
            "{key} should map to {dest}"
        );
    }
}

#[test]
fn newly_registered_auth_secrets_are_marked_secret() {
    assert!(spec_for("SOMA_MCP_AUTH_BOOTSTRAP_SECRET").unwrap().secret);
    assert!(spec_for("SOMA_MCP_TOKEN_ENCRYPTION_KEY").unwrap().secret);
    assert!(!spec_for("SOMA_MCP_AUTH_KEY_PATH").unwrap().secret);
    assert!(!spec_for("SOMA_MCP_STATIC_TOKEN_WRITE").unwrap().secret);
}
