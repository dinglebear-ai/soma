//! Tool JSON schemas for the MCP soma tool.
//!
//! This file defines the action list and input schema for the `soma` tool.
//! MCP clients inspect this schema to know what arguments are valid.
//!
//! **Customize**: rename `soma` to your tool name. Add/remove actions and
//! parameters to match your service. Use `"required": [...]` for mandatory args.

#[cfg(test)]
use std::sync::OnceLock;

use serde_json::{json, Map, Value};

use soma_provider_core::{ProviderCatalog, ProviderTool};

use crate::ACTION_DISCRIMINATOR_FIELD;

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
/// This is also used by the schema resource (`soma://schema/mcp-tool`).
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
        "name": "soma",
        "description": "Soma MCP tool demonstrating the action-based dispatch pattern. Use action=help for full documentation.",
        "x-soma-action-metadata": action_metadata(catalogs),
        "x-soma-agent-guidance": {
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
        },
        "outputSchema": structured_output_schema(catalogs)
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
                "output_schema": tool.output_schema.clone().unwrap_or(Value::Null),
            })
        })
        .collect()
}

fn structured_output_schema(catalogs: &[ProviderCatalog]) -> Value {
    let action_tools = catalogs
        .iter()
        .flat_map(|catalog| catalog.tools.iter())
        .collect::<Vec<_>>();
    let action_output_schemas = catalogs
        .iter()
        .flat_map(|catalog| catalog.tools.iter())
        .filter_map(|tool| {
            tool.output_schema.as_ref().map(|schema| {
                json!({
                    "action": tool.name,
                    "outputSchema": schema,
                })
            })
        })
        .collect::<Vec<_>>();
    let mut output_variants = action_tools
        .iter()
        .map(|tool| action_output_variant_schema(tool))
        .collect::<Vec<_>>();
    output_variants.push(response_page_output_schema());
    output_variants.push(tool_error_output_schema());

    json!({
        "type": "object",
        "description": "Structured JSON object returned in CallToolResult.structuredContent. Successful action results include _soma_action as the MCP adapter discriminator; inspect oneOf, x-soma-action-output-schemas, and x-soma-action-metadata for per-action contracts.",
        "additionalProperties": true,
        "properties": {
            ACTION_DISCRIMINATOR_FIELD: {
                "type": "string",
                "description": "MCP adapter discriminator identifying the invoked Soma action for successful non-paged tool results."
            },
            "kind": {
                "type": "string",
                "description": "Envelope kind for adapter responses such as paged output."
            }
        },
        "oneOf": output_variants,
        "x-soma-action-discriminator": ACTION_DISCRIMINATOR_FIELD,
        "x-soma-action-output-schemas": action_output_schemas,
    })
}

fn action_output_variant_schema(tool: &ProviderTool) -> Value {
    let Some(mut schema) = tool.output_schema.clone() else {
        return loose_action_output_schema(&tool.name);
    };
    if add_action_discriminator_to_schema(&mut schema, &tool.name) {
        schema
    } else {
        loose_action_output_schema(&tool.name)
    }
}

fn add_action_discriminator_to_schema(schema: &mut Value, action: &str) -> bool {
    let Some(schema_object) = schema.as_object_mut() else {
        return false;
    };
    let properties = schema_object
        .entry("properties")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(properties) = properties.as_object_mut() else {
        return false;
    };
    properties.insert(
        ACTION_DISCRIMINATOR_FIELD.to_owned(),
        json!({
            "const": action,
            "description": format!("MCP adapter discriminator for action `{action}`."),
        }),
    );
    let required = schema_object
        .entry("required")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(required) = required.as_array_mut() else {
        return false;
    };
    if !required
        .iter()
        .any(|value| value.as_str() == Some(ACTION_DISCRIMINATOR_FIELD))
    {
        required.push(Value::String(ACTION_DISCRIMINATOR_FIELD.to_owned()));
    }
    true
}

fn loose_action_output_schema(action: &str) -> Value {
    json!({
        "type": "object",
        "additionalProperties": true,
        "required": [ACTION_DISCRIMINATOR_FIELD],
        "properties": {
            ACTION_DISCRIMINATOR_FIELD: {
                "const": action,
                "description": format!("MCP adapter discriminator for action `{action}`."),
            }
        }
    })
}

fn response_page_output_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": true,
        "not": { "required": [ACTION_DISCRIMINATOR_FIELD] },
        "required": ["kind", "schema_version", "code", "content", "page"],
        "properties": {
            "kind": { "const": "mcp_response_page" },
            "schema_version": { "type": "integer" },
            "code": { "const": "response_page" },
            "message": { "type": "string" },
            "serialized_bytes": { "type": "integer" },
            "max_response_bytes": { "type": "integer" },
            "content_format": { "const": "application/json-fragment" },
            "content": { "type": "string" },
            "page": { "type": "object" },
            "continuation": {
                "type": ["object", "null"]
            }
        }
    })
}

fn tool_error_output_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": true,
        "not": { "required": [ACTION_DISCRIMINATOR_FIELD] },
        "required": ["kind", "schema_version", "code", "message"],
        "properties": {
            "kind": { "const": "mcp_tool_error" },
            "schema_version": { "type": "integer" },
            "code": { "type": "string" },
            "tool": { "type": "string" },
            "provider": { "type": "string" },
            "action": { "type": ["string", "null"] },
            "message": { "type": "string" },
            "retryable": { "type": "boolean" },
            "remediation": { "type": "string" },
            "provider_error_kind": { "type": "string" },
            "serialized_bytes": { "type": "integer" },
            "max_response_bytes": { "type": "integer" }
        }
    })
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
    STATIC_CATALOG.get_or_init(|| {
        crate::testing::loopback_state()
            .application()
            .catalog_snapshot()
            .catalogs
            .into_iter()
            .next()
            .expect("static test catalog")
    })
}

#[cfg(test)]
#[path = "schemas_tests.rs"]
mod tests;
