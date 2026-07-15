use crate::upstream::{CapScope, ToolDescriptor, UpstreamError};

use super::PoolEntry;

pub(super) fn ensure_tool_exposed(entry: &PoolEntry, tool: &str) -> Result<(), UpstreamError> {
    if !matches_filter(entry.config.expose_tools.as_deref(), tool) {
        return Err(UpstreamError::NotExposed {
            upstream: entry.snapshot.name.clone(),
            item: tool.to_owned(),
        });
    }
    if entry
        .snapshot
        .tools
        .iter()
        .any(|candidate| candidate.name == tool)
    {
        return Ok(());
    }
    Err(UpstreamError::NotExposed {
        upstream: entry.snapshot.name.clone(),
        item: tool.to_owned(),
    })
}

pub(super) fn matches_filter(filters: Option<&[String]>, candidate: &str) -> bool {
    filters.is_none_or(|filters| {
        filters.iter().any(|filter| {
            filter == candidate
                || filter == "*"
                || filter
                    .strip_suffix('*')
                    .is_some_and(|prefix| candidate.starts_with(prefix))
        })
    })
}

impl super::UpstreamPool {
    pub fn exposed_tools(&self, upstream: &str) -> Result<Vec<ToolDescriptor>, UpstreamError> {
        self.with_entry(upstream, |entry| {
            let tools: Vec<ToolDescriptor> = entry
                .snapshot
                .tools
                .iter()
                .filter(|tool| matches_filter(entry.config.expose_tools.as_deref(), &tool.name))
                .cloned()
                .collect();
            let bytes = serde_json::to_vec(&tools).map_or(usize::MAX, |bytes| bytes.len());
            self.response_caps().enforce(CapScope::ToolsList, bytes)?;
            Ok(tools)
        })
    }
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;
