use crate::upstream::{CapScope, PromptDescriptor, UpstreamError};

use super::tools::matches_filter;

impl super::UpstreamPool {
    pub fn list_prompts(&self, upstream: &str) -> Result<Vec<PromptDescriptor>, UpstreamError> {
        self.with_entry(upstream, |entry| {
            if !entry.config.proxy_prompts {
                return Ok(Vec::new());
            }
            let prompts: Vec<PromptDescriptor> = entry
                .snapshot
                .prompts
                .iter()
                .filter(|prompt| {
                    matches_filter(entry.config.expose_prompts.as_deref(), &prompt.name)
                })
                .cloned()
                .collect();
            let bytes = serde_json::to_vec(&prompts).map_or(usize::MAX, |bytes| bytes.len());
            self.response_caps().enforce(CapScope::PromptsList, bytes)?;
            Ok(prompts)
        })
    }
}

#[cfg(test)]
#[path = "prompts_tests.rs"]
mod tests;
