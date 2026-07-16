use super::*;

#[test]
fn virtual_server_defaults_keep_surfaces_disabled() {
    let cfg = VirtualServerConfig {
        id: "demo".to_owned(),
        service: "demo".to_owned(),
        ..VirtualServerConfig::default()
    };
    assert!(!cfg.enabled);
    assert!(!cfg.surfaces.cli);
    assert!(!cfg.surfaces.api);
    assert!(!cfg.surfaces.mcp);
    assert!(!cfg.surfaces.webui);
}

#[test]
fn virtual_server_requires_id_and_service() {
    assert!(VirtualServerConfig::default().validate().is_err());
}
