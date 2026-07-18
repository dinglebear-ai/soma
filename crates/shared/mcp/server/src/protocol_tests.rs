use super::*;

#[test]
fn tool_from_json_definition_preserves_output_schema() {
    let tool = tool_from_json_definition(serde_json::json!({
        "name": "soma",
        "description": "Dispatch Soma actions.",
        "inputSchema": {
            "type": "object",
            "properties": { "action": { "type": "string" } },
            "required": ["action"]
        },
        "outputSchema": {
            "type": "object",
            "additionalProperties": true,
            "properties": { "status": { "type": "string" } }
        }
    }))
    .expect("tool definition should convert");

    assert_eq!(tool.name, "soma");
    let schema = tool
        .output_schema
        .as_ref()
        .expect("outputSchema should be copied onto rmcp Tool");
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"]["status"]["type"], "string");
}

#[test]
fn tool_from_json_definition_requires_name_and_input_schema() {
    assert!(tool_from_json_definition(serde_json::json!({"description": "no name"})).is_err());
    assert!(tool_from_json_definition(serde_json::json!({"name": "x"})).is_err());
}

#[test]
fn tool_from_json_definition_rejects_non_object_output_schema() {
    let result = tool_from_json_definition(serde_json::json!({
        "name": "soma",
        "inputSchema": { "type": "object" },
        "outputSchema": "not-an-object"
    }));
    assert!(
        result.is_err(),
        "a non-object outputSchema must fail rather than being silently accepted"
    );
}

#[test]
fn tool_from_descriptor_defaults_missing_schema_to_object() {
    let tool = tool_from_descriptor("echo", None, None, None, false);

    assert_eq!(tool.input_schema["type"], "object");
    assert!(tool.output_schema.is_none());
    assert_eq!(
        tool.annotations.as_ref().and_then(|a| a.destructive_hint),
        Some(false)
    );
}

#[test]
fn tool_from_descriptor_defaults_non_object_input_schema_to_object() {
    // Documented "never fails" behavior: a malformed (non-object) input
    // schema from an upstream/route source is defaulted rather than
    // propagated as an error.
    let tool = tool_from_descriptor(
        "echo",
        None,
        Some(serde_json::json!("not-an-object")),
        None,
        false,
    );
    assert_eq!(tool.input_schema["type"], "object");
    assert_eq!(
        tool.input_schema.len(),
        1,
        "malformed schema is discarded, not merged"
    );
}

#[test]
fn tool_from_descriptor_drops_non_object_output_schema() {
    // Documented "never fails" behavior: a malformed (non-object) output
    // schema is dropped rather than propagated as an error.
    let tool = tool_from_descriptor(
        "echo",
        None,
        None,
        Some(serde_json::json!(["not", "an", "object"])),
        false,
    );
    assert!(tool.output_schema.is_none());
}

#[test]
fn tool_from_descriptor_carries_schemas_and_destructive_flag() {
    let tool = tool_from_descriptor(
        "delete_thing",
        Some("Deletes a thing".to_owned()),
        Some(serde_json::json!({"type": "object", "properties": {"id": {"type": "string"}}})),
        Some(serde_json::json!({"type": "object"})),
        true,
    );

    assert_eq!(tool.input_schema["properties"]["id"]["type"], "string");
    assert!(tool.output_schema.is_some());
    assert_eq!(
        tool.annotations.as_ref().and_then(|a| a.destructive_hint),
        Some(true)
    );
}

#[test]
fn resource_from_descriptor_builds_named_resource() {
    let resource = resource_from_descriptor("mcp-gateway://upstream/one/thing", "thing");
    assert_eq!(resource.uri, "mcp-gateway://upstream/one/thing");
    assert_eq!(resource.name, "thing");
}

#[test]
fn prompt_from_descriptor_carries_description() {
    let prompt = prompt_from_descriptor("help", Some("shows help"));
    assert_eq!(prompt.name, "help");
    assert_eq!(prompt.description.as_deref(), Some("shows help"));
}
