use crate::upstream::{
    CapScope, McpRequestOutcome, McpRoundTrip, ResourceDescriptor, UpstreamError,
};

use super::tools::matches_filter;

impl super::UpstreamPool {
    pub async fn list_resources(
        &self,
        upstream: &str,
    ) -> Result<Vec<ResourceDescriptor>, UpstreamError> {
        self.ensure_connected(upstream).await?;
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

    pub async fn read_resource(
        &self,
        upstream: &str,
        uri: &str,
    ) -> Result<serde_json::Value, UpstreamError> {
        self.ensure_connected(upstream).await?;
        let peer =
            self.with_entry(upstream, |entry| {
                if !entry.config.proxy_resources {
                    return Err(UpstreamError::Unsupported {
                        upstream: upstream.to_owned(),
                        capability: "resources/read",
                    });
                }
                entry.live.as_ref().map(|live| live.peer()).ok_or_else(|| {
                    UpstreamError::Unsupported {
                        upstream: upstream.to_owned(),
                        capability: "resources/read",
                    }
                })
            })?;
        let value = super::live::read_live_resource(upstream, peer, uri.to_owned()).await?;
        let bytes = serde_json::to_vec(&value).map_or(usize::MAX, |bytes| bytes.len());
        self.response_caps()
            .enforce(CapScope::ResourcesRead, bytes)?;
        Ok(value)
    }

    pub async fn read_resource_once(
        &self,
        upstream: &str,
        uri: &str,
        round_trip: McpRoundTrip,
    ) -> Result<McpRequestOutcome, UpstreamError> {
        self.ensure_connected(upstream).await?;
        let peer =
            self.with_entry(upstream, |entry| {
                if !entry.config.proxy_resources {
                    return Err(UpstreamError::Unsupported {
                        upstream: upstream.to_owned(),
                        capability: "resources/read",
                    });
                }
                entry.live.as_ref().map(|live| live.peer()).ok_or_else(|| {
                    UpstreamError::Unsupported {
                        upstream: upstream.to_owned(),
                        capability: "resources/read",
                    }
                })
            })?;
        let outcome =
            super::live::read_live_resource_once(upstream, peer, uri.to_owned(), round_trip)
                .await?;
        let bytes = serde_json::to_vec(outcome.payload()).map_or(usize::MAX, |bytes| bytes.len());
        self.response_caps()
            .enforce(CapScope::ResourcesRead, bytes)?;
        Ok(outcome)
    }
}

#[cfg(test)]
#[path = "resources_tests.rs"]
mod tests;
