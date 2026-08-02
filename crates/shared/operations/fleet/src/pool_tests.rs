use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::{ConnectionFactory, FleetError, HostEndpoint, SshEndpoint};

#[derive(Debug)]
struct MockConnection(usize);

#[derive(Default)]
struct MockFactory {
    connects: AtomicUsize,
    closes: AtomicUsize,
}

#[async_trait]
impl ConnectionFactory for MockFactory {
    type Connection = MockConnection;

    async fn connect(
        &self,
        _host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> FleetResult<Self::Connection> {
        if cancellation.is_cancelled() {
            return Err(FleetError::Cancelled);
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
        Ok(MockConnection(
            self.connects.fetch_add(1, Ordering::SeqCst) + 1,
        ))
    }

    async fn close(&self, _connection: &Self::Connection) -> FleetResult<()> {
        self.closes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn ssh(user: &str) -> HostRecord {
    HostRecord::new(
        HostId::new("dookie").unwrap(),
        HostEndpoint::Ssh(
            SshEndpoint::new("100.64.0.10")
                .unwrap()
                .with_user(user)
                .unwrap(),
        ),
    )
}

#[tokio::test(flavor = "current_thread")]
async fn concurrent_cold_cache_calls_share_one_connect() {
    let factory = Arc::new(MockFactory::default());
    let pool = Arc::new(ConnectionPool::new(Arc::clone(&factory)));
    let host = ssh("jmagar");
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let pool = Arc::clone(&pool);
        let host = host.clone();
        tasks.push(tokio::spawn(async move {
            pool.get_or_connect(&host, &CancellationToken::new())
                .await
                .unwrap()
        }));
    }
    let connections = futures::future::join_all(tasks)
        .await
        .into_iter()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(factory.connects.load(Ordering::SeqCst), 1);
    assert!(
        connections
            .windows(2)
            .all(|pair| Arc::ptr_eq(&pair[0], &pair[1]))
    );
    assert_eq!(connections[0].0, 1);
    assert_eq!(pool.len().await, 1);
}

#[tokio::test(flavor = "current_thread")]
async fn topology_changes_use_new_keys_and_evict_old_connections() {
    let factory = Arc::new(MockFactory::default());
    let pool = ConnectionPool::new(Arc::clone(&factory));
    let old = ssh("jmagar");
    let current = ssh("root");
    let first = pool
        .get_or_connect(&old, &CancellationToken::new())
        .await
        .unwrap();
    let second = pool
        .get_or_connect(&current, &CancellationToken::new())
        .await
        .unwrap();
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(factory.connects.load(Ordering::SeqCst), 2);

    let snapshot = TopologySnapshot::new([current.clone()]).unwrap();
    assert_eq!(pool.retain_snapshot(&snapshot).await.unwrap(), 1);
    assert_eq!(factory.closes.load(Ordering::SeqCst), 1);
    assert_eq!(pool.len().await, 1);
    assert_eq!(pool.invalidate_host(current.id()).await.unwrap(), 1);
    assert_eq!(factory.closes.load(Ordering::SeqCst), 2);
    assert!(pool.is_empty().await);
}

#[tokio::test(flavor = "current_thread")]
async fn shutdown_closes_every_initialized_connection() {
    let factory = Arc::new(MockFactory::default());
    let pool = ConnectionPool::new(Arc::clone(&factory));
    let one = HostRecord::new(HostId::new("one").unwrap(), HostEndpoint::Local);
    let two = HostRecord::new(HostId::new("two").unwrap(), HostEndpoint::Local);
    pool.get_or_connect(&one, &CancellationToken::new())
        .await
        .unwrap();
    pool.get_or_connect(&two, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(pool.shutdown().await.unwrap(), 2);
    assert_eq!(factory.closes.load(Ordering::SeqCst), 2);
    assert!(pool.is_empty().await);
}
