use crate::upstream::{UpstreamError, UpstreamHealth};

impl super::UpstreamPool {
    pub fn upstream_health(&self, upstream: &str) -> Result<UpstreamHealth, UpstreamError> {
        self.with_entry(upstream, |entry| Ok(entry.snapshot.health.clone()))
    }

    pub fn connected_count(&self) -> usize {
        self.snapshots()
            .iter()
            .filter(|snapshot| snapshot.health.is_routable())
            .count()
    }
}

#[cfg(test)]
#[path = "health_tests.rs"]
mod tests;
