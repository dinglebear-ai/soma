use super::checkout::RunnerPool;
use super::config::PoolConfig;
use super::runner_handle::RunnerSpawn;

#[tokio::test]
async fn pool_checkout_returns_lease_without_spawning_until_needed() {
    let pool = RunnerPool::new(PoolConfig::default(), RunnerSpawn::current_exe().unwrap());
    let lease = pool.checkout().await.unwrap();
    assert!(lease.handle.is_none());
    assert_eq!(pool.config().size, 2);
}
