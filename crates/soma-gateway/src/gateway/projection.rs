use crate::upstream::UpstreamHealth;

use super::manager::{GatewayManager, GatewayManagerError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayProjection {
    pub upstream_count: usize,
    pub connected_count: usize,
    pub discovered_tool_count: usize,
    pub exposed_tool_count: usize,
    pub likely_stale_count: usize,
}

impl GatewayProjection {
    pub fn from_manager(manager: &GatewayManager) -> Result<Self, GatewayManagerError> {
        let snapshots = manager.discover()?;
        let upstream_count = snapshots.len();
        let connected_count = snapshots
            .iter()
            .filter(|snapshot| snapshot.health == UpstreamHealth::Connected)
            .count();
        let discovered_tool_count = snapshots
            .iter()
            .map(|snapshot| snapshot.tools.len())
            .sum::<usize>();
        let likely_stale_count = snapshots.iter().filter(|snapshot| snapshot.stale).count();
        Ok(Self {
            upstream_count,
            connected_count,
            discovered_tool_count,
            exposed_tool_count: discovered_tool_count,
            likely_stale_count,
        })
    }
}

#[cfg(test)]
#[path = "projection_tests.rs"]
mod tests;
