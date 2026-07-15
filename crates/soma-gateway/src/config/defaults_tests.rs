use super::*;

#[test]
fn paths_use_soma_names() {
    let root = std::env::temp_dir().join("phase3-defaults").join(".soma");
    let paths = GatewayPaths::new(root.clone()).unwrap();
    assert_eq!(paths.home(), root.as_path());
    assert_eq!(paths.config_path(), root.join("config.toml"));
    assert_eq!(paths.env_path(), root.join(".env"));
}

#[test]
fn rejects_non_soma_and_relative_homes() {
    assert!(GatewayPaths::new(PathBuf::from("relative/.soma")).is_err());
    assert!(GatewayPaths::new(std::env::temp_dir().join(".labby")).is_err());
}
