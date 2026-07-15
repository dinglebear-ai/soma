use crate::gateway::manager::GatewayManager;
use crate::gateway::protected_routes::{project_route, ProtectedRouteProjection};

impl GatewayManager {
    pub fn protected_route_projections(&self) -> Vec<ProtectedRouteProjection> {
        let config = self.config.read().expect("gateway config poisoned");
        config
            .protected_mcp_routes
            .iter()
            .map(|route| project_route(route, route.upstream.as_deref().is_some()))
            .collect()
    }
}

#[cfg(test)]
#[path = "protected_routes_tests.rs"]
mod tests;
