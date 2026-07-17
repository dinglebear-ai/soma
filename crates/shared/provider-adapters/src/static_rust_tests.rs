use serde_json::json;
use soma_provider_core::{ProviderCall, ProviderManifest, ProviderSurface};

use super::*;

fn catalog(result: Option<serde_json::Value>) -> ProviderCatalog {
    let mut tool = json!({
        "name": "echo-me",
        "description": "Echo back the call.",
        "input_schema": {"type": "object"},
    });
    if let Some(result) = result {
        tool["meta"] = json!({ "result": result });
    }
    serde_json::from_value::<ProviderManifest>(json!({
        "schema_version": 1,
        "provider": { "name": "static-echo-fixture", "kind": "static-rust" },
        "tools": [tool],
    }))
    .expect("fixture manifest deserializes")
}

fn call(action: &str) -> ProviderCall {
    ProviderCall::new(action, json!({"hello": "world"})).with_surface(ProviderSurface::Mcp)
}

#[tokio::test]
async fn echoes_canned_meta_result_when_declared() {
    let provider = StaticEchoProvider::new(
        PathBuf::from("providers/echo-me.json"),
        catalog(Some(json!({"ok": true}))),
    );
    let output = provider.call(call("echo-me")).await.expect("call succeeds");
    assert_eq!(output.value, json!({"ok": true}));
}

#[tokio::test]
async fn echoes_call_shape_when_no_result_declared() {
    let provider = StaticEchoProvider::new(PathBuf::from("providers/echo-me.json"), catalog(None));
    let output = provider.call(call("echo-me")).await.expect("call succeeds");
    assert_eq!(output.value["action"], "echo-me");
    assert_eq!(output.value["params"], json!({"hello": "world"}));
}

#[tokio::test]
async fn unknown_action_is_rejected() {
    let provider = StaticEchoProvider::new(PathBuf::from("providers/echo-me.json"), catalog(None));
    let error = provider
        .call(call("does-not-exist"))
        .await
        .expect_err("unknown action should fail");
    assert_eq!(&*error.code, "unknown_file_provider_action");
}
