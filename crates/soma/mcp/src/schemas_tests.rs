use serde_json::json;
use soma_domain::actions::{action_names, action_spec};
use soma_provider_core::{ProviderIdentity, ProviderKind, ProviderManifest, ProviderTool};

use super::{tool_definitions, tool_definitions_for_catalogs};

#[test]
fn schema_action_enum_comes_from_action_metadata() {
    let tools = tool_definitions();
    let enum_values = tools[0]["inputSchema"]["properties"]["action"]["enum"]
        .as_array()
        .expect("action enum should be an array")
        .iter()
        .map(|value| value.as_str().expect("action enum values are strings"))
        .collect::<Vec<_>>();

    assert_eq!(enum_values, action_names());
}

#[test]
fn schema_advertises_action_costs_and_agent_guidance() {
    let tools = tool_definitions();
    let metadata = tools[0]["x-soma-action-metadata"]
        .as_array()
        .expect("action metadata should be an array");
    let status = metadata
        .iter()
        .find(|entry| entry["name"] == "status")
        .expect("status metadata should be present");

    assert_eq!(status["cost"], "cheap");
    assert_eq!(
        tools[0]["x-soma-agent-guidance"]["cost_order"],
        serde_json::json!(["cheap", "moderate", "expensive", "write"])
    );
    assert!(tools[0]["x-soma-agent-guidance"]["first_pass"]
        .as_array()
        .expect("first_pass should be an array")
        .contains(&serde_json::json!("status")));
    assert_eq!(status["output_schema"]["type"], "object");
}

#[test]
fn echo_message_schema_requires_non_empty_string() {
    let tools = tool_definitions();
    assert_eq!(
        tools[0]["inputSchema"]["properties"]["message"]["minLength"],
        1
    );
    assert_eq!(
        tools[0]["inputSchema"]["properties"]["message"]["description"],
        action_spec("echo").unwrap().params[0].description
    );
}

#[test]
fn schema_advertises_reserved_response_paging_args() {
    let tools = tool_definitions();
    let properties = &tools[0]["inputSchema"]["properties"];

    assert_eq!(properties["_response_offset"]["minimum"], 0);
    assert_eq!(properties["_response_page_bytes"]["minimum"], 1);
    assert_eq!(properties["_response_page_bytes"]["maximum"], 16000);
    assert_eq!(properties["_response_cursor"]["type"], "string");
}

#[test]
fn schema_conditionally_requires_echo_message() {
    let tools = tool_definitions();
    let all_of = tools[0]["inputSchema"]["allOf"]
        .as_array()
        .expect("schema should include conditional action validation");
    assert!(
        all_of.iter().any(
            |entry| entry["if"]["properties"]["action"]["const"] == "echo"
                && entry["then"]["required"]
                    .as_array()
                    .is_some_and(|required| required.iter().any(|field| field == "message"))
        ),
        "echo action must conditionally require message"
    );
}

#[test]
fn schema_mcp_only_condition_is_derived_from_action_metadata() {
    let tools = tool_definitions();
    let all_of = tools[0]["inputSchema"]["allOf"]
        .as_array()
        .expect("schema should include conditional action validation");
    assert!(
        all_of.iter().any(|entry| {
            entry["if"]["properties"]["action"]["enum"]
                .as_array()
                .is_some_and(|actions| {
                    actions.contains(&serde_json::json!("elicit_name"))
                        && actions.contains(&serde_json::json!("scaffold_intent"))
                })
        }),
        "MCP-only actions should be grouped in a derived schema condition"
    );
}

#[test]
fn schema_disallows_unknown_top_level_properties() {
    let tools = tool_definitions();
    assert_eq!(tools[0]["inputSchema"]["additionalProperties"], false);
}

#[test]
fn schema_advertises_structured_output_contract() {
    let tools = tool_definitions();
    let output_schema = &tools[0]["outputSchema"];

    assert_eq!(output_schema["type"], "object");
    assert_eq!(output_schema["additionalProperties"], true);
    assert_eq!(output_schema["x-soma-action-discriminator"], "_soma_action");
    assert!(output_schema["description"]
        .as_str()
        .expect("output schema should describe structured content")
        .contains("structuredContent"));
    assert!(output_schema["oneOf"]
        .as_array()
        .expect("output schema should include exact variants")
        .iter()
        .any(|variant| variant["properties"]["kind"]["const"] == "mcp_response_page"));
    assert!(output_schema["oneOf"]
        .as_array()
        .expect("output schema should include error variants")
        .iter()
        .any(|variant| variant["properties"]["kind"]["const"] == "mcp_tool_error"));
}

#[test]
fn schema_preserves_provider_action_output_schemas() {
    let output_schema = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["forecast"],
        "properties": {
            "forecast": { "type": "string" }
        }
    });
    let catalog = ProviderManifest {
        schema_version: 1,
        provider: ProviderIdentity {
            name: "dynamic".to_owned(),
            kind: ProviderKind::StaticRust,
            title: None,
            description: None,
            homepage: None,
            source: None,
            version: None,
            enabled: Some(true),
        },
        tools: vec![
            ProviderTool {
                name: "weather".to_owned(),
                description: "Fetch weather".to_owned(),
                title: None,
                input_schema: json!({
                    "type": "object",
                    "properties": { "city": { "type": "string" } }
                }),
                output_schema: Some(output_schema.clone()),
                scope: Some("soma:read".to_owned()),
                destructive: false,
                requires_admin: false,
                cost: Some("cheap".to_owned()),
                env: Vec::new(),
                limits: None,
                mcp: None,
                rest: None,
                cli: None,
                palette: None,
                ui: None,
                examples: Vec::new(),
                meta: json!({}),
            },
            ProviderTool {
                name: "opaque_weather".to_owned(),
                description: "Fetch opaque weather".to_owned(),
                title: None,
                input_schema: json!({
                    "type": "object",
                    "properties": { "city": { "type": "string" } }
                }),
                output_schema: None,
                scope: Some("soma:read".to_owned()),
                destructive: false,
                requires_admin: false,
                cost: Some("cheap".to_owned()),
                env: Vec::new(),
                limits: None,
                mcp: None,
                rest: None,
                cli: None,
                palette: None,
                ui: None,
                examples: Vec::new(),
                meta: json!({}),
            },
        ],
        prompts: Vec::new(),
        resources: Vec::new(),
        tasks: Vec::new(),
        elicitation: Vec::new(),
        env: Vec::new(),
        capabilities: Default::default(),
        docs: None,
        plugin: None,
        ui: None,
        meta: json!({}),
    };
    let tools = tool_definitions_for_catalogs(&[catalog]);
    let action_outputs = tools[0]["outputSchema"]["x-soma-action-output-schemas"]
        .as_array()
        .expect("aggregate output schema should include per-action schemas");
    let metadata = tools[0]["x-soma-action-metadata"]
        .as_array()
        .expect("action metadata should be present");

    assert_eq!(action_outputs[0]["action"], "weather");
    assert_eq!(action_outputs[0]["outputSchema"], output_schema);
    assert_eq!(metadata[0]["output_schema"], output_schema);
    assert_eq!(metadata[1]["output_schema"], serde_json::Value::Null);

    let variants = tools[0]["outputSchema"]["oneOf"]
        .as_array()
        .expect("aggregate output schema should include discriminated variants");
    let weather_variant = variants
        .iter()
        .find(|variant| variant["properties"]["_soma_action"]["const"] == "weather")
        .expect("weather output schema should be discriminated by action");
    assert!(weather_variant["required"]
        .as_array()
        .expect("weather variant should declare required fields")
        .contains(&json!("_soma_action")));
    assert_eq!(weather_variant["properties"]["forecast"]["type"], "string");

    let opaque_variant = variants
        .iter()
        .find(|variant| variant["properties"]["_soma_action"]["const"] == "opaque_weather")
        .expect("actions without output schemas should still get discriminator branches");
    assert_eq!(opaque_variant["additionalProperties"], true);
    assert!(opaque_variant["required"]
        .as_array()
        .expect("opaque variant should require the discriminator")
        .contains(&json!("_soma_action")));
}
