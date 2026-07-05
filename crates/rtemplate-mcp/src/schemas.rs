//! Tool JSON schemas for the MCP example tool.
//!
//! This file defines the action list and input schema for the `example` tool.
//! MCP clients inspect this schema to know what arguments are valid.
//!
//! **Template**: rename `example` to your tool name. Add/remove actions and
//! parameters to match your service. Use `"required": [...]` for mandatory args.

#[cfg(test)]
use std::sync::OnceLock;

use serde_json::{json, Map, Value};

use rtemplate_contracts::providers::{ProviderCatalog, ProviderTool};
#[cfg(test)]
use rtemplate_service::StaticRustProvider;

/// Cached JSON schema definitions (static data, built once at first call).
#[cfg(test)]
static TOOL_DEFINITIONS: OnceLock<Vec<Value>> = OnceLock::new();
#[cfg(test)]
static STATIC_CATALOG: OnceLock<ProviderCatalog> = OnceLock::new();

/// Return the JSON schema definitions for all tools (cached after first call).
///
/// Returns a `Vec<Value>` where each item is a tool definition object matching
/// the MCP `Tool` schema: `{ name, description, inputSchema }`.
///
/// This is also used by the schema resource (`example://schema/mcp-tool`).
#[cfg(test)]
pub(super) fn tool_definitions() -> &'static Vec<Value> {
    TOOL_DEFINITIONS.get_or_init(build_tool_definitions)
}

#[cfg(test)]
fn build_tool_definitions() -> Vec<Value> {
    tool_definitions_for_catalogs(std::slice::from_ref(static_catalog()))
}

pub(super) fn tool_definitions_for_catalogs(catalogs: &[ProviderCatalog]) -> Vec<Value> {
    let properties = build_input_properties(catalogs);
    let mut all_of = required_param_conditionals(catalogs);
    let mcp_only = mcp_only_action_names(catalogs);
    if !mcp_only.is_empty() {
        all_of.push(json!({
            "if": {
                "properties": {
                    "action": { "enum": mcp_only }
                },
                "required": ["action"]
            },
            "then": {
                "description": "This action uses MCP elicitation. The setup fields are requested through the client-rendered elicitation form, not through tool-call arguments."
            }
        }));
    }

    vec![json!({
        "name": "example",
        "description": "Example MCP tool demonstrating the action-based dispatch pattern. Use action=help for full documentation.",
        "x-template-action-metadata": action_metadata(catalogs),
        "x-template-agent-guidance": {
            "cost_order": ["cheap", "moderate", "expensive", "write"],
            "first_pass": ["status", "help"],
            "escalate_only_when_scoped": [],
            "default_bounds": {
                "limit": 10,
                "offset": 0
            }
        },
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": ["action"],
            "additionalProperties": false,
            "allOf": all_of
        }
    })]
}

fn action_metadata(catalogs: &[ProviderCatalog]) -> Vec<Value> {
    catalogs
        .iter()
        .flat_map(|catalog| catalog.tools.iter())
        .map(|tool| {
            json!({
                "name": tool.name,
                "cost": tool.cost.as_deref().unwrap_or("cheap"),
                "description": tool.description,
                "destructive": tool.destructive,
                "requires_admin": tool.requires_admin,
            })
        })
        .collect()
}

fn build_input_properties(catalogs: &[ProviderCatalog]) -> Map<String, Value> {
    let mut properties = Map::new();
    properties.insert(
        "action".to_owned(),
        json!({
            "type": "string",
            "description": "The operation to perform.",
            "enum": action_names(catalogs)
        }),
    );

    for tool in catalogs.iter().flat_map(|catalog| catalog.tools.iter()) {
        if let Some(params) = tool
            .input_schema
            .get("properties")
            .and_then(Value::as_object)
        {
            for (name, schema) in params {
                properties
                    .entry(name.to_owned())
                    .or_insert_with(|| schema.clone());
            }
        }
    }

    properties.insert(
        "_response_offset".to_owned(),
        json!({
            "type": "integer",
            "minimum": 0,
            "description": "Reserved MCP adapter continuation offset. Use only with _response_cursor from a prior kind=mcp_response_page response."
        }),
    );
    properties.insert(
        "_response_page_bytes".to_owned(),
        json!({
            "type": "integer",
            "minimum": 1,
            "maximum": 16000,
            "description": "Reserved MCP adapter page size in bytes. Use with _response_cursor and _response_offset to scroll cached serialized JSON responses."
        }),
    );
    properties.insert(
        "_response_cursor".to_owned(),
        json!({
            "type": "string",
            "description": "Reserved MCP adapter cursor. Required with _response_offset so continuation reads cached response data instead of re-running the action."
        }),
    );
    properties
}

fn action_names(catalogs: &[ProviderCatalog]) -> Vec<&str> {
    catalogs
        .iter()
        .flat_map(|catalog| catalog.tools.iter())
        .map(|tool| tool.name.as_str())
        .collect()
}

fn required_param_conditionals(catalogs: &[ProviderCatalog]) -> Vec<Value> {
    catalogs
        .iter()
        .flat_map(|catalog| catalog.tools.iter())
        .filter_map(|tool| {
            let required = tool
                .input_schema
                .get("required")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            (!required.is_empty()).then(|| {
                json!({
                    "if": {
                        "properties": { "action": { "const": tool.name } },
                        "required": ["action"]
                    },
                    "then": { "required": required }
                })
            })
        })
        .collect()
}

fn mcp_only_action_names(catalogs: &[ProviderCatalog]) -> Vec<&str> {
    catalogs
        .iter()
        .flat_map(|catalog| catalog.tools.iter())
        .filter(|tool| is_mcp_only(tool))
        .map(|tool| tool.name.as_str())
        .collect()
}

fn is_mcp_only(tool: &ProviderTool) -> bool {
    tool.mcp.as_ref().map(|mcp| mcp.enabled).unwrap_or(true)
        && !tool.rest.as_ref().map(|rest| rest.enabled).unwrap_or(false)
        && !tool.cli.as_ref().map(|cli| cli.enabled).unwrap_or(false)
}

#[cfg(test)]
fn static_catalog() -> &'static ProviderCatalog {
    STATIC_CATALOG.get_or_init(StaticRustProvider::catalog_static)
}

#[cfg(test)]
#[path = "schemas_tests.rs"]
mod tests;
