//! Projection of gateway routes into concrete `rmcp::model` types.
//!
//! [`super::mcp_routes`] returns loose route/descriptor structs — reused
//! directly from `soma-mcp-proxy` (`GatewayToolRoute` etc. are type aliases
//! for `soma_mcp_proxy::Mcp*Route`). A caller that wants to hand these routes
//! to its own inbound MCP clients — rather than just tracking route names —
//! needs real `rmcp::model` objects. `soma-mcp-proxy` already owns that exact
//! conversion (`rmcp_tool_from_route` and friends, itself built on
//! `soma-mcp-server`'s generic descriptor projection), so this module
//! delegates to it instead of re-deriving the conversion, keeping a single
//! owner for "route struct -> rmcp::model type" and keeping the gateway off
//! `rmcp` server-side types.

use rmcp::model::{Prompt, Resource, Tool};
use soma_mcp_proxy::{rmcp_prompt_from_route, rmcp_resource_from_route, rmcp_tool_from_route};

use super::{GatewayManager, GatewayManagerError};

impl GatewayManager {
    pub async fn rmcp_tool_routes(&self) -> Result<Vec<Tool>, GatewayManagerError> {
        self.rmcp_tool_routes_for_subject(None).await
    }

    pub async fn rmcp_tool_routes_for_subject(
        &self,
        subject: Option<&str>,
    ) -> Result<Vec<Tool>, GatewayManagerError> {
        Ok(self
            .tool_routes_for_subject(subject)
            .await?
            .iter()
            .map(rmcp_tool_from_route)
            .collect())
    }

    pub async fn rmcp_resource_routes(&self) -> Result<Vec<Resource>, GatewayManagerError> {
        self.rmcp_resource_routes_for_subject(None).await
    }

    pub async fn rmcp_resource_routes_for_subject(
        &self,
        subject: Option<&str>,
    ) -> Result<Vec<Resource>, GatewayManagerError> {
        Ok(self
            .resource_routes_for_subject(subject)
            .await?
            .iter()
            .map(rmcp_resource_from_route)
            .collect())
    }

    pub async fn rmcp_prompt_routes(&self) -> Result<Vec<Prompt>, GatewayManagerError> {
        self.rmcp_prompt_routes_for_subject(None).await
    }

    pub async fn rmcp_prompt_routes_for_subject(
        &self,
        subject: Option<&str>,
    ) -> Result<Vec<Prompt>, GatewayManagerError> {
        Ok(self
            .prompt_routes_for_subject(subject)
            .await?
            .iter()
            .map(rmcp_prompt_from_route)
            .collect())
    }
}

#[cfg(test)]
#[path = "mcp_projection_tests.rs"]
mod tests;
