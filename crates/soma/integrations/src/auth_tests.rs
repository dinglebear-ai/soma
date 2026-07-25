use super::{soma_auth_config, soma_auth_config_builder, soma_auth_env_vars};

#[test]
fn synthesized_vars_from_default_config_carry_only_auth_mode() {
    let vars = soma_auth_env_vars(&soma_config::AuthConfig::default());
    assert_eq!(
        vars,
        vec![("SOMA_MCP_AUTH_MODE".to_string(), "bearer".to_string())],
        "unset typed fields must be omitted so soma-auth applies its own defaults"
    );
}

#[test]
fn synthesized_vars_map_every_typed_field_to_its_env_key() {
    let auth = soma_config::AuthConfig {
        mode: soma_config::AuthMode::OAuth,
        public_url: Some("https://soma.example.com".to_string()),
        google_client_id: Some("g-id".to_string()),
        google_client_secret: Some("g-secret".to_string()),
        google_callback_path: Some("/auth/g/callback".to_string()),
        google_scopes: vec!["openid".to_string(), "email".to_string()],
        authelia_issuer_url: Some("https://auth.example.com".to_string()),
        authelia_client_id: Some("a-id".to_string()),
        authelia_client_secret: Some("a-secret".to_string()),
        authelia_callback_path: Some("/auth/a/callback".to_string()),
        authelia_scopes: vec!["openid".to_string(), "profile".to_string()],
        github_client_id: Some("gh-id".to_string()),
        github_client_secret: Some("gh-secret".to_string()),
        github_callback_path: Some("/auth/gh/callback".to_string()),
        github_scopes: vec!["read:user".to_string(), "user:email".to_string()],
        default_provider: Some("authelia".to_string()),
        admin_email: "admin@example.com".to_string(),
        allowed_emails: Vec::new(),
        bootstrap_secret: Some("bootstrap".to_string()),
        sqlite_path: Some("/data/auth.db".to_string()),
        key_path: Some("/data/auth-jwt.pem".to_string()),
        access_token_ttl_secs: Some(1234),
        refresh_token_ttl_secs: Some(5678),
        auth_code_ttl_secs: Some(90),
        register_rpm: Some(7),
        authorize_rpm: Some(11),
        max_pending_oauth_states: Some(42),
        allowed_client_redirect_uris: vec![
            "https://a.example.com/cb".to_string(),
            "https://b.example.com/cb".to_string(),
        ],
        token_encryption_key: Some("0".repeat(64)),
    };

    let vars = soma_auth_env_vars(&auth);
    let expected: &[(&str, &str)] = &[
        ("SOMA_MCP_AUTH_MODE", "oauth"),
        ("SOMA_MCP_PUBLIC_URL", "https://soma.example.com"),
        ("SOMA_MCP_AUTH_ADMIN_EMAIL", "admin@example.com"),
        ("SOMA_MCP_AUTH_BOOTSTRAP_SECRET", "bootstrap"),
        ("SOMA_MCP_AUTH_SQLITE_PATH", "/data/auth.db"),
        ("SOMA_MCP_AUTH_KEY_PATH", "/data/auth-jwt.pem"),
        (
            "SOMA_MCP_AUTH_ALLOWED_REDIRECT_URIS",
            "https://a.example.com/cb,https://b.example.com/cb",
        ),
        ("SOMA_MCP_GOOGLE_CLIENT_ID", "g-id"),
        ("SOMA_MCP_GOOGLE_CLIENT_SECRET", "g-secret"),
        ("SOMA_MCP_GOOGLE_CALLBACK_PATH", "/auth/g/callback"),
        ("SOMA_MCP_GOOGLE_SCOPES", "openid,email"),
        ("SOMA_MCP_AUTHELIA_ISSUER_URL", "https://auth.example.com"),
        ("SOMA_MCP_AUTHELIA_CLIENT_ID", "a-id"),
        ("SOMA_MCP_AUTHELIA_CLIENT_SECRET", "a-secret"),
        ("SOMA_MCP_AUTHELIA_CALLBACK_PATH", "/auth/a/callback"),
        ("SOMA_MCP_AUTHELIA_SCOPES", "openid,profile"),
        ("SOMA_MCP_GITHUB_CLIENT_ID", "gh-id"),
        ("SOMA_MCP_GITHUB_CLIENT_SECRET", "gh-secret"),
        ("SOMA_MCP_GITHUB_CALLBACK_PATH", "/auth/gh/callback"),
        ("SOMA_MCP_GITHUB_SCOPES", "read:user,user:email"),
        ("SOMA_MCP_AUTH_DEFAULT_PROVIDER", "authelia"),
        ("SOMA_MCP_AUTH_ACCESS_TOKEN_TTL_SECS", "1234"),
        ("SOMA_MCP_AUTH_REFRESH_TOKEN_TTL_SECS", "5678"),
        ("SOMA_MCP_AUTH_CODE_TTL_SECS", "90"),
        ("SOMA_MCP_AUTH_REGISTER_REQUESTS_PER_MINUTE", "7"),
        ("SOMA_MCP_AUTH_AUTHORIZE_REQUESTS_PER_MINUTE", "11"),
        ("SOMA_MCP_AUTH_MAX_PENDING_OAUTH_STATES", "42"),
        (
            "SOMA_MCP_TOKEN_ENCRYPTION_KEY",
            "0000000000000000000000000000000000000000000000000000000000000000",
        ),
    ];
    let expected: Vec<(String, String)> = expected
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    assert_eq!(vars, expected);
}

#[test]
fn typed_oauth_config_builds_without_process_env() {
    let auth = soma_config::AuthConfig {
        mode: soma_config::AuthMode::OAuth,
        public_url: Some("https://soma.example.com".to_string()),
        authelia_issuer_url: Some("https://auth.example.com".to_string()),
        authelia_client_id: Some("a-id".to_string()),
        authelia_client_secret: Some("a-secret".to_string()),
        admin_email: "admin@example.com".to_string(),
        ..soma_config::AuthConfig::default()
    };

    let config = soma_auth_config(&auth).expect("typed authelia-only config should build");
    assert!(matches!(config.mode, soma_auth::config::AuthMode::OAuth));
    assert_eq!(config.default_provider, "authelia");
    assert_eq!(config.authelia.client_id, "a-id");
    // Fields the typed config left unset must come from soma-auth's own
    // defaults (crates/shared/auth/src/config.rs), not soma-side copies.
    assert_eq!(config.authelia.callback_path, "/auth/authelia/callback");
    assert_eq!(config.sqlite_path.file_name().unwrap(), "auth.db");
    assert_eq!(config.register_requests_per_minute, 20);
    assert_eq!(config.access_token_ttl.as_secs(), 3600);
}

#[test]
fn applies_somas_product_auth_defaults() {
    let config = soma_auth_config_builder()
        .build_from_sources(std::iter::empty())
        .expect("builder succeeds with defaults");

    assert_eq!(config.env_prefix, "SOMA_MCP");
    assert_eq!(config.session_cookie_name, "soma_mcp_session");
    assert_eq!(config.resource_path, "/mcp");
    assert_eq!(config.default_scope, "soma:read");
    assert!(config.enable_dynamic_registration);
    assert_eq!(
        config.scopes_supported,
        vec![
            soma_domain::actions::READ_SCOPE.to_owned(),
            soma_domain::actions::WRITE_SCOPE.to_owned(),
            soma_domain::scopes::ADMIN_SCOPE.to_owned(),
        ]
    );
}
