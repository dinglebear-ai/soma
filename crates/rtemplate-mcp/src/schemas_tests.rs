use super::tool_definitions;

#[test]
fn schema_action_enum_comes_from_action_metadata() {
    let tools = tool_definitions();
    let enum_values = tools[0]["inputSchema"]["properties"]["action"]["enum"]
        .as_array()
        .expect("action enum should be an array")
        .iter()
        .map(|value| value.as_str().expect("action enum values are strings"))
        .collect::<Vec<_>>();

    assert_eq!(
        enum_values,
        rtemplate_service::action_specs()
            .iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>()
    );
}

#[test]
fn schema_advertises_action_costs_and_agent_guidance() {
    let tools = tool_definitions();
    let metadata = tools[0]["x-template-action-metadata"]
        .as_array()
        .expect("action metadata should be an array");
    let status = metadata
        .iter()
        .find(|entry| entry["name"] == "status")
        .expect("status metadata should be present");

    assert_eq!(status["cost"], "cheap");
    assert_eq!(
        tools[0]["x-template-agent-guidance"]["cost_order"],
        serde_json::json!(["cheap", "moderate", "expensive", "write"])
    );
    assert!(tools[0]["x-template-agent-guidance"]["first_pass"]
        .as_array()
        .expect("first_pass should be an array")
        .contains(&serde_json::json!("status")));
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
        rtemplate_service::action_registry()
            .action("echo")
            .unwrap()
            .params[0]
            .description
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
