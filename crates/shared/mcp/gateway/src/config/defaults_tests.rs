use super::*;

#[test]
fn paths_use_gateway_names() {
    let root = std::env::temp_dir()
        .join("phase3-defaults")
        .join(".mcp-gateway");
    let paths = GatewayPaths::new(root.clone()).unwrap();
    assert_eq!(paths.home(), root.as_path());
    assert_eq!(paths.config_path(), root.join("config.toml"));
    assert_eq!(paths.env_path(), root.join(".env"));
}

#[test]
fn env_home_accepts_parent_or_gateway_dir() {
    let parent = std::env::temp_dir().join("phase3-env-home");
    assert_eq!(
        normalize_env_gateway_home(parent.clone()),
        parent.join(".mcp-gateway")
    );

    let gateway_home = parent.join(".mcp-gateway");
    assert_eq!(
        normalize_env_gateway_home(gateway_home.clone()),
        gateway_home
    );
}

#[test]
fn rejects_non_gateway_and_relative_homes() {
    assert!(GatewayPaths::new(PathBuf::from("relative/.mcp-gateway")).is_err());
    assert!(GatewayPaths::new(std::env::temp_dir().join(".labby")).is_err());
}
