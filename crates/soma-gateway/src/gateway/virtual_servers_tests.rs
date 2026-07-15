use crate::config::virtual_servers::VirtualServerSurfacesConfig;
use crate::config::VirtualServerConfig;

use super::*;

#[test]
fn virtual_server_projection_is_health_aware() {
    let server = VirtualServerConfig {
        id: "agent".to_owned(),
        service: "soma".to_owned(),
        enabled: true,
        surfaces: VirtualServerSurfacesConfig {
            mcp: true,
            ..VirtualServerSurfacesConfig::default()
        },
        mcp_policy: None,
    };

    let connected = project_virtual_server(&server, true);
    let disconnected = project_virtual_server(&server, false);

    assert!(connected.connected);
    assert!(!disconnected.connected);
    assert!(connected.mcp_enabled);
}
