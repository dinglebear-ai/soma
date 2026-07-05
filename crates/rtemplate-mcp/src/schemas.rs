//! Tool JSON schemas for the MCP example tool.
//!
//! This file defines the action list and input schema for the `example` tool.
//! MCP clients inspect this schema to know what arguments are valid.
//!
//! **Template**: rename `example` to your tool name. Add/remove actions and
//! parameters to match your service. Use `"required": [...]` for mandatory args.

use std::sync::OnceLock;

use serde_json::{json, Map, Value};

use rtemplate_contracts::actions::ActionTransport;

/// Cached JSON schema definitions (static data, built once at first call).
static TOOL_DEFINITIONS: OnceLock<Vec<Value>> = OnceLock::new();

/// Return the JSON schema definitions for all tools (cached after first call).
///
/// Returns a `Vec<Value>` where each item is a tool definition object matching
/// the MCP `Tool` schema: `{ name, description, inputSchema }`.
///
/// This is also used by the schema resource (`example://schema/mcp-tool`).
pub(super) fn tool_definitions() -> &'static Vec<Value> {
    TOOL_DEFINITIONS.get_or_init(build_tool_definitions)
}

fn build_tool_definitions() -> Vec<Value> {
    let properties = build_input_properties();
    let mut all_of = required_param_conditionals();
    let mcp_only = mcp_only_action_names();
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
        "x-template-action-metadata": action_metadata(),
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

fn action_metadata() -> Vec<Value> {
    rtemplate_service::action_specs()
        .iter()
        .map(|spec| {
            json!({
                "name": spec.name,
                "cost": spec.cost.as_str(),
                "description": spec.description,
                "destructive": spec.destructive,
                "requires_admin": spec.requires_admin,
            })
        })
        .collect()
}

fn build_input_properties() -> Map<String, Value> {
    let mut properties = Map::new();
    properties.insert(
        "action".to_owned(),
        json!({
            "type": "string",
            "description": "The operation to perform.",
            "enum": rtemplate_service::action_specs().iter().map(|spec| spec.name).collect::<Vec<_>>()
        }),
    );

    for spec in rtemplate_service::action_specs() {
        for param in spec.params {
            properties
                .entry(param.name.to_owned())
                .or_insert_with(|| param_schema(param));
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

fn param_schema(param: &rtemplate_contracts::actions::ParamSpec) -> Value {
    let json_type = param.ty.json_schema_type();
    let mut schema = json!({
        "type": json_type,
        "description": param.description,
    });
    if param.required && json_type == "string" {
        schema["minLength"] = json!(1);
    }
    if let Some(max_len) = param.max_len {
        schema["maxLength"] = json!(max_len);
    }
    if !param.enum_values.is_empty() {
        schema["enum"] = json!(param.enum_values);
    }
    schema
}

fn required_param_conditionals() -> Vec<Value> {
    rtemplate_service::action_specs()
        .iter()
        .filter_map(|spec| {
            let required = spec
                .params
                .iter()
                .filter(|param| param.required)
                .map(|param| Value::String(param.name.to_owned()))
                .collect::<Vec<_>>();
            (!required.is_empty()).then(|| {
                json!({
                    "if": {
                        "properties": { "action": { "const": spec.name } },
                        "required": ["action"]
                    },
                    "then": { "required": required }
                })
            })
        })
        .collect()
}

fn mcp_only_action_names() -> Vec<&'static str> {
    rtemplate_service::action_specs()
        .iter()
        .filter(|spec| spec.transport == ActionTransport::McpOnly)
        .map(|spec| spec.name)
        .collect()
}

#[cfg(test)]
#[path = "schemas_tests.rs"]
mod tests;
