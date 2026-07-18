//! Projection of proxy routes into concrete `rmcp::model` types.
//!
//! [`crate::McpToolRoute`], [`crate::McpResourceRoute`], and
//! [`crate::McpPromptRoute`] carry the loose descriptor fields recovered from
//! an upstream MCP server. Any caller that wants to hand these routes back to
//! *its own* inbound MCP clients (as opposed to just tracking route names)
//! eventually needs them as real `rmcp::model` objects. This module owns that
//! conversion once, on top of `soma-mcp-server`'s generic descriptor
//! projection, so callers do not each reimplement the JSON-schema-to-`Tool`
//! shuffle.

use rmcp::model::{Prompt, Resource, Tool};
use soma_mcp_server::protocol::{
    prompt_from_descriptor, resource_from_descriptor, tool_from_descriptor,
};

use crate::{McpPromptRoute, McpResourceRoute, McpToolRoute};

/// Project a [`McpToolRoute`] into an [`rmcp::model::Tool`] ready for
/// `tools/list`.
pub fn rmcp_tool_from_route(route: &McpToolRoute) -> Tool {
    tool_from_descriptor(
        route.name.clone(),
        route.descriptor.description.clone(),
        route.descriptor.input_schema.clone(),
        route.descriptor.output_schema.clone(),
        route.descriptor.destructive,
    )
}

/// Project a [`McpResourceRoute`] into an [`rmcp::model::Resource`] ready for
/// `resources/list`.
pub fn rmcp_resource_from_route(route: &McpResourceRoute) -> Resource {
    let name = route
        .descriptor
        .name
        .clone()
        .unwrap_or_else(|| route.native_uri.clone());
    resource_from_descriptor(route.uri.clone(), name)
}

/// Project a [`McpPromptRoute`] into an [`rmcp::model::Prompt`] ready for
/// `prompts/list`.
pub fn rmcp_prompt_from_route(route: &McpPromptRoute) -> Prompt {
    prompt_from_descriptor(route.name.clone(), route.descriptor.description.as_deref())
}

#[cfg(test)]
#[path = "projection_tests.rs"]
mod tests;
