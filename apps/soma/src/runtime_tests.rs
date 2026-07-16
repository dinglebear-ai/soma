use super::*;

#[test]
fn local_and_stdio_modes_default_to_quiet_logging() {
    assert_eq!(default_log_level(true, false), "warn");
    assert_eq!(default_log_level(false, false), "warn");
}

#[test]
fn http_server_mode_defaults_to_info_logging() {
    assert_eq!(default_log_level(false, true), "info");
}

#[cfg(feature = "auth")]
#[test]
fn soma_auth_config_builder_supports_upstream_oauth_without_inbound_oauth() {
    let cfg = soma_mcp_auth_config_builder()
        .build_from_sources([
            (
                "SOMA_MCP_PUBLIC_URL".to_string(),
                "https://mcp.example.com".to_string(),
            ),
            (
                "SOMA_MCP_AUTH_SQLITE_PATH".to_string(),
                "/tmp/soma-auth.db".to_string(),
            ),
        ])
        .unwrap();

    assert!(matches!(cfg.mode, soma_auth::config::AuthMode::Bearer));
    assert_eq!(cfg.resource_path, "/mcp");
    assert!(cfg.scopes_supported.contains(&"soma:admin".to_string()));
}
