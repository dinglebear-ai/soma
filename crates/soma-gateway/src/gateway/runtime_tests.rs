use crate::config::GatewayConfig;

use super::*;

#[test]
fn runtime_exposes_one_shared_manager_handle() {
    let runtime = GatewayRuntime::new(GatewayConfig::default()).unwrap();
    let left = runtime.manager();
    let right = runtime.manager();

    assert!(std::sync::Arc::ptr_eq(&left, &right));
}
