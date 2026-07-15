use crate::config::GatewayConfig;
use crate::gateway::manager::GatewayManager;

use super::*;

#[test]
fn read_access_can_list_but_cannot_admin_test() {
    let manager = GatewayManager::new(GatewayConfig::default()).unwrap();
    let read = GatewayAccess {
        read: true,
        admin: false,
    };

    dispatch_gateway_action(&manager, read, "gateway.list", serde_json::json!({})).unwrap();
    let error = dispatch_gateway_action(
        &manager,
        read,
        "gateway.test",
        serde_json::json!({"command": "node"}),
    )
    .unwrap_err();

    assert!(matches!(error, GatewayDispatchError::AdminRequired));
}

#[test]
fn admin_spawn_actions_run_spawn_validation() {
    let manager = GatewayManager::new(GatewayConfig::default()).unwrap();
    let admin = GatewayAccess {
        read: true,
        admin: true,
    };
    let error = dispatch_gateway_action(
        &manager,
        admin,
        "gateway.test",
        serde_json::json!({"command": "/tmp/x/node"}),
    )
    .unwrap_err();

    assert!(matches!(error, GatewayDispatchError::SpawnValidation));
}

#[test]
fn structured_errors_keep_stable_gateway_shape() {
    let error = GatewayDispatchError::Params(ParamsError::MustBeObject);
    let structured = error.structured("gateway.add").to_json();

    assert_eq!(structured["code"], "invalid_param");
    assert_eq!(structured["action"], "gateway.add");
}
