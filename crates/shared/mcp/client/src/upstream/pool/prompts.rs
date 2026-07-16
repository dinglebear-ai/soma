use crate::upstream::{CapScope, PromptDescriptor, UpstreamError};

use super::tools::matches_filter;

impl super::UpstreamPool {
    pub async fn list_prompts(
        &self,
        upstream: &str,
    ) -> Result<Vec<PromptDescriptor>, UpstreamError> {
        self.ensure_connected(upstream).await?;
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

    pub async fn get_prompt(
        &self,
        upstream: &str,
        name: &str,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<serde_json::Value, UpstreamError> {
        self.ensure_connected(upstream).await?;
        let peer =
            self.with_entry(upstream, |entry| {
                if !entry.config.proxy_prompts {
                    return Err(UpstreamError::Unsupported {
                        upstream: upstream.to_owned(),
                        capability: "prompts/get",
                    });
                }
                entry.live.as_ref().map(|live| live.peer()).ok_or_else(|| {
                    UpstreamError::Unsupported {
                        upstream: upstream.to_owned(),
                        capability: "prompts/get",
                    }
                })
            })?;
        let value =
            super::live::get_live_prompt(upstream, peer, name.to_owned(), arguments).await?;
        let bytes = serde_json::to_vec(&value).map_or(usize::MAX, |bytes| bytes.len());
        self.response_caps().enforce(CapScope::PromptsGet, bytes)?;
        Ok(value)
    }
}

#[cfg(test)]
#[path = "prompts_tests.rs"]
mod tests;
