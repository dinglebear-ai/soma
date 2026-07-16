use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use super::{RelayCapabilities, RelaySessionId};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelayCacheKey {
    pub upstream: String,
    pub session_id: RelaySessionId,
    pub subject: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayConnection {
    pub key: RelayCacheKey,
    pub created_at: Instant,
    pub last_used: Instant,
    pub alive: bool,
    pub capabilities: RelayCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayConnectSlot {
    CacheHit,
    Leader,
    Waiter,
}

#[derive(Debug, Clone)]
pub struct RelayCache {
    entries: BTreeMap<RelayCacheKey, RelayConnection>,
    connect_locks: BTreeSet<RelayCacheKey>,
    ttl: Duration,
    capacity: usize,
    pending_shutdown: Vec<RelayConnection>,
}

impl RelayCache {
    #[must_use]
    pub fn new(ttl: Duration, capacity: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            connect_locks: BTreeSet::new(),
            ttl,
            capacity: capacity.max(1),
            pending_shutdown: Vec::new(),
        }
    }

    pub fn begin_connect(&mut self, key: RelayCacheKey, now: Instant) -> RelayConnectSlot {
        self.sweep(now);
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used = now;
            return RelayConnectSlot::CacheHit;
        }
        if !self.connect_locks.insert(key) {
            return RelayConnectSlot::Waiter;
        }
        RelayConnectSlot::Leader
    }

    pub fn complete_connect(&mut self, connection: RelayConnection) {
        self.connect_locks.remove(&connection.key);
        self.entries.insert(connection.key.clone(), connection);
        self.evict_over_capacity();
    }

    pub fn leader_cancelled(&mut self, key: &RelayCacheKey) {
        self.connect_locks.remove(key);
    }

    pub fn connect_failed(&mut self, key: &RelayCacheKey) {
        self.connect_locks.remove(key);
    }

    pub fn waiter_cancelled(&mut self, _key: &RelayCacheKey) {}

    pub fn mark_dead(&mut self, key: &RelayCacheKey) {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.alive = false;
        }
    }

    pub fn sweep(&mut self, now: Instant) {
        let expired: Vec<RelayCacheKey> = self
            .entries
            .iter()
            .filter(|(_, entry)| !entry.alive || now.duration_since(entry.last_used) >= self.ttl)
            .map(|(key, _)| key.clone())
            .collect();
        for key in expired {
            if let Some(connection) = self.entries.remove(&key) {
                self.pending_shutdown.push(connection);
            }
        }
    }

    pub fn lock_count(&self) -> usize {
        self.connect_locks.len()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn take_pending_shutdown(&mut self) -> Vec<RelayConnection> {
        std::mem::take(&mut self.pending_shutdown)
    }

    fn evict_over_capacity(&mut self) {
        while self.entries.len() > self.capacity {
            let Some(lru_key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                return;
            };
            if let Some(connection) = self.entries.remove(&lru_key) {
                self.pending_shutdown.push(connection);
            }
        }
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
