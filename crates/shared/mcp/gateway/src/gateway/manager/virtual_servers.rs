use crate::gateway::manager::GatewayManager;
use crate::gateway::virtual_servers::{project_virtual_server, VirtualServerProjection};

impl GatewayManager {
    pub fn virtual_server_projections(&self) -> Vec<VirtualServerProjection> {
        let config = self.config.read().expect("gateway config poisoned");
        config
            .virtual_servers
            .iter()
            .map(|server| project_virtual_server(server, false))
            .collect()
    }
}

#[cfg(test)]
#[path = "virtual_servers_tests.rs"]
mod tests;
