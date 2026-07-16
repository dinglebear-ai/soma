use super::*;
use crate::config::{ProtectedMcpRouteConfig, UpstreamConfig, VirtualServerConfig};

#[test]
fn installs_default_config_when_missing() {
    let dir = tempfile::tempdir().unwrap();
    let store = FsGatewayConfigStore::new(dir.path().join(".mcp-gateway"));
    let cfg = store.load_or_install_default().unwrap();
    assert_eq!(cfg, GatewayConfig::default());
    assert!(store.paths().config_path().exists());
}

#[test]
fn toml_round_trip_preserves_gateway_sections() {
    let dir = tempfile::tempdir().unwrap();
    let store = FsGatewayConfigStore::new(dir.path().join(".mcp-gateway"));
    let cfg = GatewayConfig {
        upstream: vec![UpstreamConfig {
            name: "demo".to_owned(),
            url: Some("https://example.com/mcp".to_owned()),
            bearer_token_env: Some("DEMO_TOKEN".to_owned()),
            ..UpstreamConfig::default()
        }],
        protected_mcp_routes: vec![ProtectedMcpRouteConfig {
            name: "route".to_owned(),
            public_host: "mcp.example.com".to_owned(),
            public_path: "/demo".to_owned(),
            upstream: Some("demo".to_owned()),
            ..ProtectedMcpRouteConfig::default()
        }],
        virtual_servers: vec![VirtualServerConfig {
            id: "demo".to_owned(),
            service: "demo".to_owned(),
            enabled: true,
            ..VirtualServerConfig::default()
        }],
    };
    store.save(&cfg).unwrap();
    assert_eq!(store.load().unwrap(), cfg);
}

#[test]
fn env_secret_write_merges_and_quotes_values() {
    let dir = tempfile::tempdir().unwrap();
    let store = FsGatewayConfigStore::new(dir.path().join(".mcp-gateway"));
    store
        .write_env_secret("DEMO_TOKEN", "secret value")
        .unwrap();
    store.write_env_secret("OTHER_TOKEN", "abc").unwrap();
    let raw = fs::read_to_string(store.paths().env_path()).unwrap();
    assert!(raw.contains("DEMO_TOKEN=\"secret value\""));
    assert!(raw.contains("OTHER_TOKEN=abc"));
}

#[cfg(unix)]
#[test]
fn env_secret_write_uses_0600_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let store = FsGatewayConfigStore::new(dir.path().join(".mcp-gateway"));
    store.write_env_secret("DEMO_TOKEN", "secret").unwrap();
    let mode = fs::metadata(store.paths().env_path())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}
