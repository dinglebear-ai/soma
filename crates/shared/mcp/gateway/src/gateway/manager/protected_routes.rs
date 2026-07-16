use std::cmp::Reverse;

use crate::gateway::manager::GatewayManager;
use crate::gateway::protected_routes::{project_route, route_matches, ProtectedRouteProjection};
use crate::upstream::UpstreamHealth;

impl GatewayManager {
    pub fn protected_route_list(&self) -> Vec<crate::config::ProtectedMcpRouteConfig> {
        self.config
            .read()
            .expect("gateway config poisoned")
            .protected_mcp_routes
            .clone()
    }

    pub fn resolve_protected_route(
        &self,
        host: &str,
        path: &str,
    ) -> Option<crate::config::ProtectedMcpRouteConfig> {
        let mut routes = self.protected_route_list();
        routes.sort_by_key(|route| Reverse(route.public_path.len()));
        routes
            .into_iter()
            .filter(|route| route.enabled)
            .find(|route| route_matches(route, host, path).is_ok())
    }

    pub fn resolve_protected_route_metadata(
        &self,
        host: &str,
        path: &str,
    ) -> Option<crate::config::ProtectedMcpRouteConfig> {
        const PREFIX: &str = "/.well-known/oauth-protected-resource";
        let suffix = path.strip_prefix(PREFIX)?;
        let public_path = if suffix.is_empty() { "/mcp" } else { suffix };
        self.protected_route_list()
            .into_iter()
            .filter(|route| route.enabled && route.public_path == public_path)
            .find(|route| route_matches(route, host, &route.public_path).is_ok())
    }

    pub async fn protected_route_projections(&self) -> Vec<ProtectedRouteProjection> {
        let routes = self
            .config
            .read()
            .expect("gateway config poisoned")
            .protected_mcp_routes
            .clone();
        let snapshots = self.discover().await.unwrap_or_default();
        routes
            .iter()
            .map(|route| {
                let upstream_connected = route.upstream.as_deref().is_some_and(|name| {
                    snapshots.iter().any(|snapshot| {
                        snapshot.name == name && snapshot.health == UpstreamHealth::Connected
                    })
                });
                project_route(route, upstream_connected)
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "protected_routes_tests.rs"]
mod tests;
