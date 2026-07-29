use std::collections::BTreeMap;

use rmcp::model::{CancelTaskParams, GetTaskParams, UpdateTaskParams};
use serde_json::Value;

use crate::upstream::{CapScope, UpstreamError};

use super::{UpstreamPool, ensure_routable};

impl UpstreamPool {
    pub async fn get_task(&self, upstream: &str, task_id: &str) -> Result<Value, UpstreamError> {
        self.ensure_connected(upstream).await?;
        let peer = self.task_peer(upstream)?;
        let result = peer
            .get_task(GetTaskParams::new(task_id))
            .await
            .map_err(|error| task_error(upstream, "tasks/get", error))?;
        let value = serde_json::to_value(result)
            .map_err(|error| task_error(upstream, "tasks/get", error))?;
        let bytes = serde_json::to_vec(&value).map_or(usize::MAX, |bytes| bytes.len());
        self.response_caps().enforce(CapScope::ToolsCall, bytes)?;
        Ok(value)
    }

    pub async fn update_task(
        &self,
        upstream: &str,
        task_id: &str,
        input_responses: BTreeMap<String, Value>,
    ) -> Result<(), UpstreamError> {
        self.ensure_connected(upstream).await?;
        let peer = self.task_peer(upstream)?;
        peer.update_task(UpdateTaskParams::new(task_id, input_responses))
            .await
            .map_err(|error| task_error(upstream, "tasks/update", error))
    }

    pub async fn cancel_task(&self, upstream: &str, task_id: &str) -> Result<(), UpstreamError> {
        self.ensure_connected(upstream).await?;
        let peer = self.task_peer(upstream)?;
        peer.cancel_task(CancelTaskParams::new(task_id))
            .await
            .map_err(|error| task_error(upstream, "tasks/cancel", error))
    }

    fn task_peer(
        &self,
        upstream: &str,
    ) -> Result<rmcp::service::Peer<rmcp::RoleClient>, UpstreamError> {
        self.with_entry(upstream, |entry| {
            ensure_routable(entry)?;
            entry
                .live
                .as_ref()
                .map(|live| live.peer())
                .ok_or_else(|| UpstreamError::Unsupported {
                    upstream: upstream.to_owned(),
                    capability: "tasks",
                })
        })
    }
}

fn task_error(
    upstream: &str,
    operation: &'static str,
    error: impl std::fmt::Display,
) -> UpstreamError {
    UpstreamError::LiveCall {
        upstream: upstream.to_owned(),
        operation,
        message: error.to_string(),
    }
}

#[cfg(test)]
#[path = "tasks_tests.rs"]
mod tests;
