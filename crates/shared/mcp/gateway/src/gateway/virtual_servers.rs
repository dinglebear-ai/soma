use crate::config::VirtualServerConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualServerProjection {
    pub id: String,
    pub service: String,
    pub enabled: bool,
    pub connected: bool,
    pub mcp_enabled: bool,
}

pub fn project_virtual_server(
    server: &VirtualServerConfig,
    service_connected: bool,
) -> VirtualServerProjection {
    VirtualServerProjection {
        id: server.id.clone(),
        service: server.service.clone(),
        enabled: server.enabled,
        connected: server.enabled && service_connected,
        mcp_enabled: server.surfaces.mcp,
    }
}

#[cfg(test)]
#[path = "virtual_servers_tests.rs"]
mod tests;
