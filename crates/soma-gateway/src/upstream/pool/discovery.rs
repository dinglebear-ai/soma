use crate::upstream::{CapScope, UpstreamError, UpstreamSnapshot};

use super::UpstreamPool;

impl UpstreamPool {
    pub fn discover(&self) -> Result<Vec<UpstreamSnapshot>, UpstreamError> {
        let snapshots = self.snapshots();
        let bytes = serde_json::to_vec(&snapshots).map_or(usize::MAX, |bytes| bytes.len());
        self.response_caps().enforce(CapScope::ToolsList, bytes)?;
        Ok(snapshots)
    }

    pub fn discover_upstream(&self, upstream: &str) -> Result<UpstreamSnapshot, UpstreamError> {
        self.with_entry(upstream, |entry| Ok(entry.snapshot.clone()))
    }

    #[must_use]
    pub fn subject_scoped_discovery_limit(&self) -> usize {
        self.discovery_concurrency()
    }
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
