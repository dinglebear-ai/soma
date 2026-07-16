use serde_json::{json, Value};

use crate::gateway::manager::{GatewayManager, GatewayManagerError};
use crate::gateway::projection::GatewayProjection;

pub async fn gateway_list_view(manager: &GatewayManager) -> Result<Value, GatewayManagerError> {
    let projection = GatewayProjection::from_manager(manager).await?;
    Ok(json!({
        "upstream_count": projection.upstream_count,
        "connected_count": projection.connected_count,
        "discovered_tool_count": projection.discovered_tool_count,
        "exposed_tool_count": projection.exposed_tool_count,
        "likely_stale_count": projection.likely_stale_count,
    }))
}

pub fn gateway_config_view(manager: &GatewayManager) -> Value {
    serde_json::to_value(manager.config_view()).expect("gateway config view serializes")
}

#[cfg(test)]
#[path = "view_models_tests.rs"]
mod tests;
