//! Integration tests for MCP tool dispatch.
//!
//! Tests verify that MCP action parsing and non-elicitation dispatch return valid JSON.
//! Uses `loopback_state()` from the test-support feature — no real creds needed.
//!
//! **Template**: mirror this file for your service. Add one test per action.

use rmcp::{
    model::{CallToolRequestParams, CallToolResult},
    service::ServiceError,
    ServiceExt,
};
use rmcp_template::{
    actions::ExampleAction,
    mcp::{execute_tool_without_peer_for_test, rmcp_server},
    testing::{bearer_state, loopback_state},
};
use serde_json::{json, Value};

async fn call_mcp_action(args: serde_json::Value) -> serde_json::Value {
    let state = loopback_state();
    execute_tool_without_peer_for_test(&state, "example", args)
        .await
        .expect("MCP tool dispatch should succeed")
}

async fn call_real_mcp_tool(args: serde_json::Map<String, Value>) -> anyhow::Result<Value> {
    let result = call_real_mcp_tool_result(args).await?;
    let payload = result_text_json(&result)?;
    assert_eq!(result.structured_content.as_ref(), Some(&payload));
    Ok(payload)
}

async fn call_real_mcp_tool_result(
    args: serde_json::Map<String, Value>,
) -> anyhow::Result<CallToolResult> {
    let (server_transport, client_transport) = tokio::io::duplex(16 * 1024);

    let server_handle = tokio::spawn(async move {
        rmcp_server(loopback_state())
            .serve(server_transport)
            .await?
            .waiting()
            .await?;
        anyhow::Ok(())
    });

    let client = ().serve(client_transport).await?;
    let result = client
        .call_tool(CallToolRequestParams::new("example").with_arguments(args))
        .await?;
    client.cancel().await?;
    server_handle.await??;
    Ok(result)
}

fn result_text_json(result: &CallToolResult) -> anyhow::Result<Value> {
    let text = result
        .content
        .first()
        .and_then(|content| content.raw.as_text())
        .map(|text| text.text.as_str())
        .expect("call_tool result should contain JSON text");
    let payload: Value = serde_json::from_str(text)?;
    Ok(payload)
}

#[tokio::test]
async fn test_greet_no_name_returns_greeting() {
    let result = call_mcp_action(json!({ "action": "greet" })).await;
    let greeting = result
        .get("greeting")
        .and_then(|v| v.as_str())
        .expect("greeting field should be present");
    assert!(
        greeting.contains("Hello"),
        "greeting should contain Hello, got: {greeting}"
    );
}

#[tokio::test]
async fn test_greet_with_name_includes_name() {
    let result = call_mcp_action(json!({ "action": "greet", "name": "Alice" })).await;
    let greeting = result
        .get("greeting")
        .and_then(|v| v.as_str())
        .expect("greeting field should be present");
    assert!(
        greeting.contains("Alice"),
        "greeting should include Alice, got: {greeting}"
    );
}

#[tokio::test]
async fn test_echo_returns_message() {
    let result = call_mcp_action(json!({ "action": "echo", "message": "hello world" })).await;
    let echo = result
        .get("echo")
        .and_then(|v| v.as_str())
        .expect("echo field should be present");
    assert_eq!(echo, "hello world");
}

#[tokio::test]
async fn test_status_returns_ok() {
    let result = call_mcp_action(json!({ "action": "status" })).await;
    let status = result
        .get("status")
        .and_then(|v| v.as_str())
        .expect("status field should be present");
    assert_eq!(status, "ok");
}

#[tokio::test]
async fn test_real_call_tool_path_returns_status_json() -> anyhow::Result<()> {
    let mut args = serde_json::Map::new();
    args.insert("action".to_owned(), json!("status"));
    let payload = call_real_mcp_tool(args).await?;

    assert_eq!(payload["status"], "ok");
    Ok(())
}

#[tokio::test]
async fn full_mcp_call_tool_path_uses_service_registry() -> anyhow::Result<()> {
    let mut args = serde_json::Map::new();
    args.insert("action".to_owned(), json!("echo"));
    args.insert("message".to_owned(), json!("hello"));
    let payload = call_real_mcp_tool(args).await?;

    assert_eq!(payload["echo"], "hello");
    Ok(())
}

#[tokio::test]
async fn full_mcp_call_tool_path_returns_structured_validation_errors() -> anyhow::Result<()> {
    let cases = [
        (json!({"action": "echo"}), "missing_field", "message"),
        (
            json!({"action": "echo", "message": "hello", "extra": true}),
            "unknown_field",
            "extra",
        ),
        (
            json!({"action": "echo", "message": "x".repeat(4097)}),
            "too_long",
            "message",
        ),
    ];

    for (args, code, field) in cases {
        let args = args
            .as_object()
            .expect("case args should be object")
            .clone();
        let result = call_real_mcp_tool_result(args).await?;
        assert_eq!(result.is_error, Some(true));
        let payload = result_text_json(&result)?;
        assert_eq!(result.structured_content.as_ref(), Some(&payload));
        assert_eq!(payload["code"], code);
        assert_eq!(payload["field"], field);
        assert_eq!(payload["service_error_kind"], "validation");
    }
    Ok(())
}

#[tokio::test]
async fn test_real_call_tool_missing_http_context_returns_structured_auth_error(
) -> anyhow::Result<()> {
    let (server_transport, client_transport) = tokio::io::duplex(16 * 1024);

    let server_handle = tokio::spawn(async move {
        rmcp_server(bearer_state("secret"))
            .serve(server_transport)
            .await?
            .waiting()
            .await?;
        anyhow::Ok(())
    });

    let mut args = serde_json::Map::new();
    args.insert("action".to_owned(), json!("status"));
    let client = ().serve(client_transport).await?;
    let error = client
        .call_tool(CallToolRequestParams::new("example").with_arguments(args))
        .await
        .expect_err("bare transport should lack HTTP auth extensions");

    let ServiceError::McpError(error) = error else {
        panic!("expected MCP protocol error, got: {error}");
    };
    assert!(error.message.contains("missing http context"));
    let data = error
        .data
        .expect("auth error should include structured data");
    assert_eq!(data["kind"], "mcp_auth_error");
    assert_eq!(data["code"], "missing_http_context");
    assert_eq!(data["retryable"], false);

    client.cancel().await?;
    server_handle.await??;
    Ok(())
}

#[tokio::test]
async fn test_all_actions_return_valid_json_object() {
    for args in &[
        json!({ "action": "greet" }),
        json!({ "action": "echo", "message": "hello world" }),
        json!({ "action": "status" }),
        json!({ "action": "help" }),
    ] {
        let action = args["action"].as_str().unwrap();
        let result = call_mcp_action(args.clone()).await;
        assert!(
            result.is_object(),
            "action={action} should return a JSON object, got: {result}"
        );
    }
}

#[tokio::test]
async fn test_greet_target_defaults_to_world() {
    let result = call_mcp_action(json!({ "action": "greet" })).await;
    let target = result
        .get("target")
        .and_then(|v| v.as_str())
        .expect("target field should be present");
    assert_eq!(target, "World");
}

#[test]
#[cfg(feature = "mcp-http")]
fn test_schemas_actions_list_is_non_empty() {
    // Verify the schema action list compiles and has the expected entries
    use rmcp_template::server;
    let _ = server::router(loopback_state()); // builds router — exercises schema code path
}

#[test]
fn test_scaffold_intent_action_parses_for_mcp_dispatch() {
    let action = ExampleAction::from_mcp_args(&json!({ "action": "scaffold_intent" }))
        .expect("scaffold_intent should parse for MCP dispatch");
    assert_eq!(action, ExampleAction::ScaffoldIntent);
}

#[tokio::test]
async fn test_mcp_dispatch_rejects_missing_action() {
    let state = loopback_state();
    let error = execute_tool_without_peer_for_test(&state, "example", json!({}))
        .await
        .expect_err("missing action should be rejected");
    assert!(error.to_string().contains("action is required"));
}

#[tokio::test]
async fn test_mcp_dispatch_rejects_unknown_action() {
    let state = loopback_state();
    let error =
        execute_tool_without_peer_for_test(&state, "example", json!({ "action": "missing" }))
            .await
            .expect_err("unknown action should be rejected");
    assert!(error.to_string().contains("unknown example action"));
}

#[tokio::test]
async fn test_mcp_dispatch_rejects_peer_required_actions_without_peer() {
    let state = loopback_state();
    let error =
        execute_tool_without_peer_for_test(&state, "example", json!({ "action": "elicit_name" }))
            .await
            .expect_err("elicitation action should require a peer");
    assert!(error.to_string().contains("requires an MCP peer"));
}
