use soma_mcp_client::upstream::{PromptDescriptor, ResourceDescriptor, ToolDescriptor};

use super::*;
use crate::{resource_route, tool_routes_from_candidates};

#[test]
fn rmcp_tool_from_route_carries_schema_and_destructive_flag() {
    let routes = tool_routes_from_candidates(
        vec![(
            "alpha".to_owned(),
            ToolDescriptor {
                name: "delete_thing".to_owned(),
                description: Some("deletes a thing".to_owned()),
                input_schema: Some(serde_json::json!({"type": "object"})),
                output_schema: None,
                destructive: true,
            },
        )],
        std::iter::empty::<&str>(),
    );

    let tool = rmcp_tool_from_route(&routes[0]);
    assert_eq!(tool.name, "delete_thing");
    assert_eq!(tool.description.as_deref(), Some("deletes a thing"));
    assert_eq!(
        tool.annotations
            .as_ref()
            .and_then(|annotations| annotations.destructive_hint),
        Some(true)
    );
}

#[test]
fn rmcp_tool_from_route_carries_output_schema() {
    let routes = tool_routes_from_candidates(
        vec![(
            "alpha".to_owned(),
            ToolDescriptor {
                name: "summarize".to_owned(),
                description: None,
                input_schema: Some(serde_json::json!({"type": "object"})),
                output_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": { "summary": { "type": "string" } }
                })),
                destructive: false,
            },
        )],
        std::iter::empty::<&str>(),
    );

    let tool = rmcp_tool_from_route(&routes[0]);
    let output_schema = tool
        .output_schema
        .as_ref()
        .expect("output_schema should propagate through the route projection");
    assert_eq!(output_schema["properties"]["summary"]["type"], "string");
}

#[test]
fn rmcp_resource_from_route_falls_back_to_native_uri_when_unnamed() {
    let route = resource_route(
        "up.one",
        ResourceDescriptor {
            uri: "native://thing".to_owned(),
            name: None,
        },
    );

    let resource = rmcp_resource_from_route(&route);
    assert_eq!(resource.name, "native://thing");
}

#[test]
fn rmcp_prompt_from_route_carries_description() {
    let routes = crate::prompt_routes_from_candidates(vec![(
        "one".to_owned(),
        PromptDescriptor {
            name: "help".to_owned(),
            description: Some("shows help".to_owned()),
        },
    )]);

    let prompt = rmcp_prompt_from_route(&routes[0]);
    assert_eq!(prompt.name, "help");
    assert_eq!(prompt.description.as_deref(), Some("shows help"));
}
