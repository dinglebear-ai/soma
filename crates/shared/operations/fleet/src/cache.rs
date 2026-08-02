use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{HostId, HostRecord, PoolKey, TopologySnapshot};

/// Thread-safe connection cache bound to exact host topology revisions.
///
/// The cache does not open or close connections. Drivers own those lifecycle
/// actions and receive removed handles from invalidation and eviction methods.
pub struct ConnectionCache<C> {
    entries: RwLock<BTreeMap<PoolKey, Arc<C>>>,
}

impl<C> Default for ConnectionCache<C> {
    fn default() -> Self {
        Self {
            entries: RwLock::new(BTreeMap::new()),
        }
    }
}

impl<C> ConnectionCache<C> {
    /// Creates an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a connection for one exact host revision.
    ///
    /// Returns the replaced handle when the same key already existed.
    pub fn insert(&self, host: &HostRecord, connection: C) -> Option<Arc<C>> {
        write_unpoisoned(&self.entries).insert(host.pool_key(), Arc::new(connection))
    }

    /// Returns a connection only when the host revision matches exactly.
    #[must_use]
    pub fn get(&self, host: &HostRecord) -> Option<Arc<C>> {
        read_unpoisoned(&self.entries)
            .get(&host.pool_key())
            .map(Arc::clone)
    }

    /// Removes one exact host revision.
    pub fn remove(&self, host: &HostRecord) -> Option<Arc<C>> {
        write_unpoisoned(&self.entries).remove(&host.pool_key())
    }

    /// Removes every cached revision for one host.
    pub fn invalidate_host(&self, host: &HostId) -> Vec<Arc<C>> {
        let mut entries = write_unpoisoned(&self.entries);
        let keys = entries
            .keys()
            .filter(|key| key.host() == host)
            .cloned()
            .collect::<Vec<_>>();
        keys.into_iter()
            .filter_map(|key| entries.remove(&key))
            .collect()
    }

    /// Removes keys absent from the supplied topology snapshot.
    ///
    /// A host whose endpoint changed has a new revision and therefore evicts
    /// the old cached connection even when its stable host identity is unchanged.
    pub fn retain_snapshot(&self, snapshot: &TopologySnapshot) -> Vec<Arc<C>> {
        let current = snapshot
            .hosts()
            .map(HostRecord::pool_key)
            .collect::<BTreeSet<_>>();
        let mut entries = write_unpoisoned(&self.entries);
        let stale = entries
            .keys()
            .filter(|key| !current.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        stale
            .into_iter()
            .filter_map(|key| entries.remove(&key))
            .collect()
    }

    /// Returns the number of cached revision keys.
    #[must_use]
    pub fn len(&self) -> usize {
        read_unpoisoned(&self.entries).len()
    }

    /// Returns whether the cache has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        read_unpoisoned(&self.entries).is_empty()
    }

    /// Returns sorted cached keys for metrics or diagnostics.
    #[must_use]
    pub fn keys(&self) -> Vec<PoolKey> {
        read_unpoisoned(&self.entries).keys().cloned().collect()
    }
}

fn read_unpoisoned<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write_unpoisoned<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
