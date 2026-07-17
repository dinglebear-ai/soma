use super::soma_auth_config_builder;

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
            soma_contracts::actions::READ_SCOPE.to_owned(),
            soma_contracts::actions::WRITE_SCOPE.to_owned(),
            soma_contracts::scopes::ADMIN_SCOPE.to_owned(),
        ]
    );
}
