use super::checkout::RunnerPool;
use super::config::PoolConfig;
use super::runner_handle::RunnerSpawn;

#[tokio::test]
async fn pool_checkout_returns_runner_handle() {
    let pool = RunnerPool::new(PoolConfig::default(), RunnerSpawn::current_exe().unwrap());
    let lease = pool.checkout().await.unwrap();
    assert!(lease.handle.is_some());
    assert_eq!(pool.config().size, 2);
}

#[tokio::test]
async fn pool_reuses_successful_runner_until_recycle_threshold() {
    let config = PoolConfig {
        size: 1,
        recycle_after: 2,
        max_overflow: 0,
    };
    let pool = RunnerPool::new(config, RunnerSpawn::current_exe().unwrap());
    let first = pool.checkout().await.unwrap();
    let first_pid = first.handle.as_ref().and_then(|handle| handle.child_pid);

    pool.release(first, super::disposition::RunnerDisposition::Reuse)
        .await;
    let second = pool.checkout().await.unwrap();
    let second_pid = second.handle.as_ref().and_then(|handle| handle.child_pid);

    assert_eq!(first_pid, second_pid);
}
