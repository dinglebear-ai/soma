//! Smoke test for the deprecated facade: confirms `soma_contracts::config`
//! still resolves to the real `soma_config` implementation. Full behavioral
//! coverage lives in `soma-config`.

use super::*;

#[test]
fn facade_reexports_the_real_config_defaults() {
    let config = McpConfig::default();
    assert_eq!(config.port, 40060);
    assert!(config.is_loopback());
}
