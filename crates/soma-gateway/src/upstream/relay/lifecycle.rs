use std::time::Instant;

use super::cache::{RelayCache, RelayConnection};

pub fn sweep_and_collect_shutdowns(cache: &mut RelayCache, now: Instant) -> Vec<RelayConnection> {
    cache.sweep(now);
    cache.take_pending_shutdown()
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
