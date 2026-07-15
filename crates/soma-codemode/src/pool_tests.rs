use super::pool::{PoolConfig, RunnerDisposition};

#[test]
fn pool_reexports_config_and_disposition() {
    assert!(!PoolConfig::default().is_disabled());
    assert_eq!(
        RunnerDisposition::from_success_count(1, 1),
        RunnerDisposition::Recycle
    );
}
