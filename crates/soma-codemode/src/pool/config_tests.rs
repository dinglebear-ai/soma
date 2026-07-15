use super::config::{PoolConfig, POOL_SIZE_ENV};

#[test]
fn default_pool_config_is_conservative() {
    let config = PoolConfig::default();
    assert_eq!(config.size, 2);
    assert!(!config.is_disabled());
    assert_eq!(POOL_SIZE_ENV, "SOMA_CODE_MODE_POOL_SIZE");
}
