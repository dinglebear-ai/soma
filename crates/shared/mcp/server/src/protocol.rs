//! Reusable protocol conversion helpers between loose JSON/descriptor shapes
//! and concrete `rmcp::model` types.
//!
//! Any inbound MCP server ends up doing this conversion at least twice: once
//! for its own natively-defined tools (usually stored as plain JSON schema
//! documents) and once for anything it re-projects from elsewhere (an
//! upstream catalog, a provider registry, a gateway route). This module owns
//! the generic half of both conversions; callers own the descriptor/schema
//! source.

use std::{borrow::Cow, sync::Arc};

use rmcp::{
    model::{Prompt, Resource, Tool, ToolAnnotations},
    ErrorData,
};
use serde_json::{Map, Value};

/// Convert an untyped MCP tool-definition JSON object
/// (`{name, description, inputSchema, outputSchema}`) into an
/// [`rmcp::model::Tool`].
///
/// This is the shape MCP clients expect from `tools/list`. It is the
/// conversion a server needs when its tool catalog is stored as plain JSON
/// (for example, provider-derived schemas) rather than built with the
/// `rmcp-macros` tool-router machinery.
pub fn tool_from_json_definition(value: Value) -> Result<Tool, ErrorData> {
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| ErrorData::internal_error("tool definition missing name", None))?;
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .map(|d| Cow::Owned(d.to_string()));
    let input_schema = value
        .get("inputSchema")
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| ErrorData::internal_error("tool definition missing inputSchema", None))?;
    let mut tool = Tool::new_with_raw(
        Cow::Owned(name.to_string()),
        description,
        Arc::new(input_schema),
    );
    if let Some(output_schema) = value.get("outputSchema") {
        let output_schema = output_schema.as_object().cloned().ok_or_else(|| {
            ErrorData::internal_error("tool outputSchema must be an object", None)
        })?;
        tool = tool.with_raw_output_schema(Arc::new(output_schema));
    }
    Ok(tool)
}

/// Build an [`rmcp::model::Tool`] from loose descriptor fields, defaulting a
/// missing or non-object input schema to an empty JSON object schema and
/// dropping a non-object output schema rather than failing.
///
/// Unlike [`tool_from_json_definition`], this never fails — it is meant for
/// building tools from already-typed route/descriptor structs (upstream
/// catalogs, gateway routes) where a malformed schema is a data-quality issue
/// upstream, not a reason to drop the whole tool.
pub fn tool_from_descriptor(
    name: impl Into<Cow<'static, str>>,
    description: Option<String>,
    input_schema: Option<Value>,
    output_schema: Option<Value>,
    destructive: bool,
) -> Tool {
    let mut tool = Tool::new_with_raw(
        name.into(),
        description.map(Cow::Owned),
        schema_object(input_schema),
    );
    if let Some(output_schema) = schema_object_opt(output_schema) {
        tool = tool.with_raw_output_schema(output_schema);
    }
    tool.with_annotations(ToolAnnotations::new().destructive(destructive))
}

/// Build an [`rmcp::model::Resource`] from a resolved URI and display name.
pub fn resource_from_descriptor(uri: impl Into<String>, name: impl Into<String>) -> Resource {
    Resource::new(uri.into(), name.into())
}

/// Build an [`rmcp::model::Prompt`] from a name and optional description,
/// with no declared arguments.
pub fn prompt_from_descriptor(name: impl Into<String>, description: Option<&str>) -> Prompt {
    Prompt::new(name.into(), description, None)
}

fn schema_object(value: Option<Value>) -> Arc<Map<String, Value>> {
    schema_object_opt(value).unwrap_or_else(|| {
        Arc::new(Map::from_iter([(
            "type".to_owned(),
            Value::String("object".to_owned()),
        )]))
    })
}

fn schema_object_opt(value: Option<Value>) -> Option<Arc<Map<String, Value>>> {
    match value {
        Some(Value::Object(map)) => Some(Arc::new(map)),
        Some(other) => {
            // A schema was supplied but is not a JSON object — a data-quality
            // issue in the upstream/provider/route source, not a reason to
            // fail the whole conversion (see `tool_from_descriptor`'s doc
            // comment). Still worth a log: silently dropping it would leave
            // no trace of a malformed upstream schema.
            tracing::warn!(
                schema.type = %schema_type_name(&other),
                "MCP tool schema is not a JSON object; dropping it"
            );
            None
        }
        None => None,
    }
}

fn schema_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
