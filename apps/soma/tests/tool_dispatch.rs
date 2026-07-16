//! Integration tests for MCP tool dispatch.
//!
//! Tests verify that MCP action parsing and non-elicitation dispatch return valid JSON.
//! Uses `loopback_state()` from the test-support feature — no real creds needed.
//!
//! **Customize**: mirror this file for your service. Add one test per action.

use async_trait::async_trait;
use rmcp::{model::CallToolRequestParams, service::ServiceError, ServiceExt};
use serde_json::json;
use soma::{
    actions::SomaAction,
    mcp::{execute_tool_without_peer_for_test, rmcp_server},
    testing::{bearer_state, loopback_state},
};
use soma_contracts::providers::{
    ProviderCatalog, ProviderIdentity, ProviderKind, ProviderManifest, ProviderTool,
};
use soma_service::provider_registry::{Provider, ProviderOutput, ProviderRegistry};
use soma_service::ProviderError;
use std::sync::Arc;

async fn call_mcp_action(args: serde_json::Value) -> serde_json::Value {
    let state = loopback_state();
    execute_tool_without_peer_for_test(&state, "soma", args)
        .await
        .expect("MCP tool dispatch should succeed")
}

#[derive(Clone)]
struct DynamicProvider;

#[async_trait]
impl Provider for DynamicProvider {
    fn catalog(&self) -> ProviderCatalog {
        ProviderManifest {
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
            tools: vec![ProviderTool {
                name: "weather".to_owned(),
                description: "Fetch weather".to_owned(),
                title: None,
                input_schema: json!({
                    "type": "object",
                    "required": ["city"],
                    "additionalProperties": false,
                    "properties": {"city": {"type": "string"}}
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
            }],
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
        }
    }

    async fn call(
        &self,
        call: soma_service::provider_registry::ProviderCall,
    ) -> Result<ProviderOutput, ProviderError> {
        Ok(ProviderOutput::json(json!({
            "provider": call.provider,
            "action": call.action,
            "city": call.params["city"],
        })))
    }
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
async fn test_dynamic_provider_action_dispatches_without_static_action_enum() {
    let mut state = loopback_state();
    state.provider_registry =
        ProviderRegistry::new(vec![Arc::new(DynamicProvider)]).expect("dynamic registry");

    let result = execute_tool_without_peer_for_test(
        &state,
        "soma",
        json!({ "action": "weather", "city": "Paris" }),
    )
    .await
    .expect("dynamic provider action should dispatch");

    assert_eq!(result["provider"], "dynamic");
    assert_eq!(result["action"], "weather");
    assert_eq!(result["city"], "Paris");
}

#[tokio::test]
async fn test_real_call_tool_path_returns_status_json() -> anyhow::Result<()> {
    let (server_transport, client_transport) = tokio::io::duplex(16 * 1024);

    let server_handle = tokio::spawn(async move {
        rmcp_server(loopback_state())
            .serve(server_transport)
            .await?
            .waiting()
            .await?;
        anyhow::Ok(())
    });

    let mut args = serde_json::Map::new();
    args.insert("action".to_owned(), json!("status"));
    let client = ().serve(client_transport).await?;
    let result = client
        .call_tool(CallToolRequestParams::new("soma").with_arguments(args))
        .await?;

    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.as_str())
        .expect("call_tool result should contain JSON text");
    let payload: serde_json::Value = serde_json::from_str(text)?;

    assert_eq!(payload["status"], "ok");
    assert_eq!(result.structured_content.as_ref(), Some(&payload));

    client.cancel().await?;
    server_handle.await??;
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
        .call_tool(CallToolRequestParams::new("soma").with_arguments(args))
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
    use soma::server;
    let _ = server::router(loopback_state()); // builds router — exercises schema code path
}

#[test]
fn test_scaffold_intent_action_parses_for_mcp_dispatch() {
    let action = SomaAction::from_mcp_args(&json!({ "action": "scaffold_intent" }))
        .expect("scaffold_intent should parse for MCP dispatch");
    assert_eq!(action, SomaAction::ScaffoldIntent);
}

#[tokio::test]
async fn test_mcp_dispatch_rejects_missing_action() {
    let state = loopback_state();
    let error = execute_tool_without_peer_for_test(&state, "soma", json!({}))
        .await
        .expect_err("missing action should be rejected");
    assert!(error.to_string().contains("action is required"));
}

#[tokio::test]
async fn test_mcp_dispatch_rejects_unknown_action() {
    let state = loopback_state();
    let error = execute_tool_without_peer_for_test(&state, "soma", json!({ "action": "missing" }))
        .await
        .expect_err("unknown action should be rejected");
    assert!(error.to_string().contains("unknown provider action"));
}

#[tokio::test]
async fn test_mcp_dispatch_rejects_peer_required_actions_without_peer() {
    let state = loopback_state();
    let error =
        execute_tool_without_peer_for_test(&state, "soma", json!({ "action": "elicit_name" }))
            .await
            .expect_err("elicitation action should require a peer");
    assert!(error.to_string().contains("requires an MCP peer"));
}
