use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelaySessionId(u64);

impl RelaySessionId {
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct RelaySessionMint {
    next: Arc<AtomicU64>,
}

impl Default for RelaySessionMint {
    fn default() -> Self {
        Self::new()
    }
}

impl RelaySessionMint {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next: Arc::new(AtomicU64::new(1)),
        }
    }

    #[must_use]
    pub fn mint(&self) -> RelaySessionId {
        RelaySessionId(self.next.fetch_add(1, Ordering::Relaxed))
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
