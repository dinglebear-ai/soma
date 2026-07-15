use crate::config::virtual_servers::VirtualServerSurfacesConfig;
use crate::config::{GatewayConfig, VirtualServerConfig};
use crate::gateway::manager::GatewayManager;

#[test]
fn manager_projects_virtual_servers() {
    let manager = GatewayManager::new(GatewayConfig {
        virtual_servers: vec![VirtualServerConfig {
            id: "agent".to_owned(),
            service: "soma".to_owned(),
            enabled: true,
            surfaces: VirtualServerSurfacesConfig {
                mcp: true,
                ..VirtualServerSurfacesConfig::default()
            },
            mcp_policy: None,
        }],
        ..GatewayConfig::default()
    })
    .unwrap();

    let projection = manager.virtual_server_projections();

    assert_eq!(projection[0].id, "agent");
    assert!(projection[0].connected);
}
