use super::config_store::FsGatewayConfigStore;

#[test]
fn config_store_is_reexported_from_gateway_module() {
    let root = std::env::temp_dir().join("soma-gateway-test").join(".soma");
    let store = FsGatewayConfigStore::new(root);
    assert!(store.paths().config_path().ends_with("config.toml"));
}
