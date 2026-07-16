use crate::config::GatewayConfig;
use crate::gateway::manager::GatewayManager;

use super::*;

const READ_ACCESS: GatewayAccess = GatewayAccess {
    read: true,
    admin: false,
};
const ADMIN_ACCESS: GatewayAccess = GatewayAccess {
    read: true,
    admin: true,
};

fn default_manager() -> GatewayManager {
    GatewayManager::new(GatewayConfig::default()).unwrap()
}

fn python_command() -> String {
    std::env::var("SOMA_PYTHON_COMMAND")
        .ok()
        .and_then(|value| bare_command_name(&value))
        .unwrap_or_else(default_python_command)
}

fn bare_command_name(value: &str) -> Option<String> {
    value
        .trim()
        .trim_matches('"')
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

fn default_python_command() -> String {
    if cfg!(windows) {
        "python".to_owned()
    } else {
        "python3".to_owned()
    }
}
#[tokio::test]
async fn read_access_can_list_but_cannot_admin_test() {
    let manager = default_manager();

    dispatch_gateway_action(&manager, READ_ACCESS, "gateway.list", serde_json::json!({}))
        .await
        .unwrap();
    let error = dispatch_gateway_action(
        &manager,
        READ_ACCESS,
        "gateway.test",
        serde_json::json!({"command": "node"}),
    )
    .await
    .unwrap_err();

    assert!(matches!(error, GatewayDispatchError::AdminRequired));
}

#[tokio::test]
async fn oauth_actions_are_registered_and_require_admin() {
    let manager = default_manager();

    let error = dispatch_gateway_action(
        &manager,
        READ_ACCESS,
        "gateway.oauth.status",
        serde_json::json!({"upstream": "drive"}),
    )
    .await
    .unwrap_err();

    assert!(matches!(error, GatewayDispatchError::AdminRequired));
}

#[tokio::test]
async fn oauth_actions_return_runtime_error_when_unconfigured() {
    let manager = default_manager();

    let error = dispatch_gateway_action(
        &manager,
        ADMIN_ACCESS,
        "gateway.oauth.status",
        serde_json::json!({"upstream": "drive"}),
    )
    .await
    .unwrap_err();

    assert!(matches!(
        error,
        GatewayDispatchError::Manager(crate::gateway::manager::GatewayManagerError::OAuth(_))
    ));
}

#[tokio::test]
async fn admin_spawn_actions_run_spawn_validation() {
    let manager = default_manager();
    let error = dispatch_gateway_action(
        &manager,
        ADMIN_ACCESS,
        "gateway.test",
        serde_json::json!({"command": "/tmp/x/node"}),
    )
    .await
    .unwrap_err();

    assert!(matches!(error, GatewayDispatchError::SpawnValidation));
}

#[tokio::test]
async fn gateway_test_connects_and_discovers_stdio_upstream() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("probe.py");
    std::fs::write(&script, STDIO_PROBE_SERVER).unwrap();
    let manager = default_manager();

    let result = dispatch_gateway_action(
        &manager,
        ADMIN_ACCESS,
        "gateway.test",
        serde_json::json!({
            "name": "probe",
            "command": python_command(),
            "args": [script.to_string_lossy()]
        }),
    )
    .await
    .unwrap();

    assert_eq!(result["ok"], true);
    assert_eq!(result["tool_count"], 1);
}

#[tokio::test]
async fn admin_spawn_actions_validate_args_and_env() {
    let manager = default_manager();

    let bad_env = dispatch_gateway_action(
        &manager,
        ADMIN_ACCESS,
        "gateway.add",
        serde_json::json!({"name": "bad", "command": "node", "env": {"LD_PRELOAD": "x.so"}}),
    )
    .await
    .unwrap_err();
    assert!(matches!(bad_env, GatewayDispatchError::SpawnValidation));

    let bad_arg = dispatch_gateway_action(
        &manager,
        ADMIN_ACCESS,
        "gateway.add",
        serde_json::json!({"name": "bad", "command": "node", "args": ["--disable-spawn-guard"]}),
    )
    .await
    .unwrap_err();
    assert!(matches!(bad_arg, GatewayDispatchError::SpawnValidation));
}

#[tokio::test]
async fn admin_actions_mutate_gateway_config() {
    let manager = default_manager();

    let added = dispatch_gateway_action(
        &manager,
        ADMIN_ACCESS,
        "gateway.add",
        serde_json::json!({"name": "demo", "url": "https://example.com/mcp"}),
    )
    .await
    .unwrap();
    assert_eq!(added["added"], true);
    assert_eq!(
        dispatch_gateway_action(
            &manager,
            ADMIN_ACCESS,
            "gateway.list",
            serde_json::json!({})
        )
        .await
        .unwrap()["upstream_count"],
        1
    );

    let removed = dispatch_gateway_action(
        &manager,
        ADMIN_ACCESS,
        "gateway.remove",
        serde_json::json!({"name": "demo"}),
    )
    .await
    .unwrap();
    assert_eq!(removed["removed"], "demo");
    assert!(manager.discover().await.unwrap().is_empty());
}

#[tokio::test]
async fn import_approve_adds_validated_upstream_config() {
    let manager = default_manager();

    let approved = dispatch_gateway_action(
        &manager,
        ADMIN_ACCESS,
        "gateway.import.approve",
        serde_json::json!({"name": "imported", "url": "https://example.com/mcp"}),
    )
    .await
    .unwrap();

    assert_eq!(approved["approved"], true);
    assert_eq!(manager.config_view().upstream[0].name, "imported");
}

#[tokio::test]
async fn unknown_actions_fail_closed() {
    let manager = default_manager();

    let error = dispatch_gateway_action(
        &manager,
        ADMIN_ACCESS,
        "gateway.typo",
        serde_json::json!({}),
    )
    .await
    .unwrap_err();

    assert!(matches!(error, GatewayDispatchError::UnknownAction));
    assert_eq!(
        error.structured("gateway.typo").to_json()["code"],
        "unknown_action"
    );
}

const STDIO_PROBE_SERVER: &str = r#"
import json
import sys

def send(id, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": id, "result": result}) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    msg = json.loads(line)
    method = msg.get("method")
    id = msg.get("id")
    if method == "initialize":
        send(id, {
            "protocolVersion": "2025-06-18",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "probe", "version": "0.0.0"}
        })
    elif method == "notifications/initialized":
        pass
    elif method == "tools/list":
        send(id, {"tools": [{
            "name": "probe",
            "inputSchema": {"type": "object"}
        }]})
    else:
        sys.stdout.write(json.dumps({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32601, "message": "Method not found"}
        }) + "\n")
        sys.stdout.flush()
"#;

#[tokio::test]
async fn structured_errors_keep_stable_gateway_shape() {
    let error = GatewayDispatchError::Params(ParamsError::MustBeObject);
    let structured = error.structured("gateway.add").to_json();

    assert_eq!(structured["code"], "invalid_param");
    assert_eq!(structured["action"], "gateway.add");
}
