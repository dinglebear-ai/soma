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
