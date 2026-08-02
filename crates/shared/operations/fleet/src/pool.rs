use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use tokio::sync::{Mutex, OnceCell};
use tokio_util::sync::CancellationToken;

use crate::{ConnectionFactory, FleetResult, HostId, HostRecord, PoolKey, TopologySnapshot};

type ConnectionCell<C> = Arc<OnceCell<Arc<C>>>;
type ConnectionCells<C> = BTreeMap<PoolKey, ConnectionCell<C>>;

/// Async connection pool keyed by stable host identity and exact topology revision.
pub struct ConnectionPool<F>
where
    F: ConnectionFactory,
{
    factory: Arc<F>,
    cells: Mutex<ConnectionCells<F::Connection>>,
}

impl<F> ConnectionPool<F>
where
    F: ConnectionFactory,
{
    /// Creates an empty pool using the supplied connection factory.
    #[must_use]
    pub fn new(factory: Arc<F>) -> Self {
        Self {
            factory,
            cells: Mutex::new(BTreeMap::new()),
        }
    }

    /// Returns an existing exact-revision connection or opens it once.
    ///
    /// Concurrent cold-cache callers for the same revision share one
    /// initialization cell. The map lock is never held across an await.
    pub async fn get_or_connect(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> FleetResult<Arc<F::Connection>> {
        let key = host.pool_key();
        let cell = {
            let mut cells = self.cells.lock().await;
            Arc::clone(
                cells
                    .entry(key.clone())
                    .or_insert_with(|| Arc::new(OnceCell::new())),
            )
        };

        let result = cell
            .get_or_try_init(|| async {
                let connection = self.factory.connect(host, cancellation).await?;
                Ok::<Arc<F::Connection>, crate::FleetError>(Arc::new(connection))
            })
            .await;

        match result {
            Ok(connection) => Ok(Arc::clone(connection)),
            Err(error) => {
                let mut cells = self.cells.lock().await;
                if cells
                    .get(&key)
                    .is_some_and(|current| Arc::ptr_eq(current, &cell) && current.get().is_none())
                {
                    cells.remove(&key);
                }
                Err(error)
            }
        }
    }

    /// Invalidates every cached revision for one host and closes each handle.
    pub async fn invalidate_host(&self, host: &HostId) -> FleetResult<usize> {
        let removed = {
            let mut cells = self.cells.lock().await;
            let keys = cells
                .keys()
                .filter(|key| key.host() == host)
                .cloned()
                .collect::<Vec<_>>();
            keys.into_iter()
                .filter_map(|key| cells.remove(&key))
                .collect::<Vec<_>>()
        };
        self.close_cells(removed).await
    }

    /// Evicts connections absent from the current topology snapshot.
    pub async fn retain_snapshot(&self, snapshot: &TopologySnapshot) -> FleetResult<usize> {
        let current = snapshot
            .hosts()
            .map(HostRecord::pool_key)
            .collect::<BTreeSet<_>>();
        let removed = {
            let mut cells = self.cells.lock().await;
            let keys = cells
                .keys()
                .filter(|key| !current.contains(*key))
                .cloned()
                .collect::<Vec<_>>();
            keys.into_iter()
                .filter_map(|key| cells.remove(&key))
                .collect::<Vec<_>>()
        };
        self.close_cells(removed).await
    }

    /// Closes and removes every initialized connection.
    pub async fn shutdown(&self) -> FleetResult<usize> {
        let removed = {
            let mut cells = self.cells.lock().await;
            std::mem::take(&mut *cells)
                .into_values()
                .collect::<Vec<_>>()
        };
        self.close_cells(removed).await
    }

    /// Returns cached revision-key count, including in-flight initializations.
    pub async fn len(&self) -> usize {
        self.cells.lock().await.len()
    }

    /// Returns whether no revision keys are cached.
    pub async fn is_empty(&self) -> bool {
        self.cells.lock().await.is_empty()
    }

    async fn close_cells(&self, cells: Vec<ConnectionCell<F::Connection>>) -> FleetResult<usize> {
        let mut closed = 0;
        for cell in cells {
            if let Some(connection) = cell.get() {
                self.factory.close(connection.as_ref()).await?;
                closed += 1;
            }
        }
        Ok(closed)
    }
}

#[cfg(test)]
#[path = "pool_tests.rs"]
mod tests;
