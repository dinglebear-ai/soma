use crate::upstream::{CapScope, ResourceDescriptor, UpstreamError};

use super::tools::matches_filter;

impl super::UpstreamPool {
    pub fn list_resources(&self, upstream: &str) -> Result<Vec<ResourceDescriptor>, UpstreamError> {
        self.with_entry(upstream, |entry| {
            if !entry.config.proxy_resources {
                return Ok(Vec::new());
            }
            let resources: Vec<ResourceDescriptor> = entry
                .snapshot
                .resources
                .iter()
                .filter(|resource| {
                    matches_filter(entry.config.expose_resources.as_deref(), &resource.uri)
                })
                .cloned()
                .collect();
            let bytes = serde_json::to_vec(&resources).map_or(usize::MAX, |bytes| bytes.len());
            self.response_caps()
                .enforce(CapScope::ResourcesList, bytes)?;
            Ok(resources)
        })
    }
}

#[cfg(test)]
#[path = "resources_tests.rs"]
mod tests;
