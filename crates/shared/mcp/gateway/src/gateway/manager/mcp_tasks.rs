use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::Ordering},
};

use serde_json::Value;

use crate::upstream::{McpRequestOutcome, UpstreamError};

use super::{GatewayManager, GatewayManagerError, TaskRoute};

impl GatewayManager {
    pub(super) fn register_task_outcome(
        &self,
        outcome: McpRequestOutcome,
        upstream: &str,
        subject: Option<&str>,
    ) -> Result<McpRequestOutcome, GatewayManagerError> {
        let McpRequestOutcome::Task(mut value) = outcome else {
            return Ok(outcome);
        };
        let native_task_id = value
            .get("taskId")
            .and_then(Value::as_str)
            .filter(|task_id| !task_id.is_empty())
            .ok_or_else(|| {
                GatewayManagerError::InvalidTaskResult(
                    "task response did not contain a non-empty taskId".to_owned(),
                )
            })?
            .to_owned();
        let public_task_id = format!(
            "soma-task-{:016x}",
            self.task_sequence.fetch_add(1, Ordering::Relaxed)
        );
        self.task_routes
            .write()
            .expect("gateway task routes poisoned")
            .insert(
                public_task_id.clone(),
                TaskRoute {
                    upstream: upstream.to_owned(),
                    native_task_id,
                    subject: subject.map(str::to_owned),
                },
            );
        value["taskId"] = Value::String(public_task_id);
        Ok(McpRequestOutcome::Task(value))
    }

    pub async fn get_mcp_task_for_subject(
        &self,
        task_id: &str,
        subject: Option<&str>,
    ) -> Result<Value, GatewayManagerError> {
        let route = self.resolve_task_route(task_id, subject)?;
        let pool = self.task_pool()?;
        let mut value = get_native_task(&pool, &route).await?;
        value["taskId"] = Value::String(task_id.to_owned());
        Ok(value)
    }

    pub async fn update_mcp_task_for_subject(
        &self,
        task_id: &str,
        input_responses: BTreeMap<String, Value>,
        subject: Option<&str>,
    ) -> Result<(), GatewayManagerError> {
        let route = self.resolve_task_route(task_id, subject)?;
        let pool = self.task_pool()?;
        update_native_task(&pool, &route, input_responses).await?;
        Ok(())
    }

    pub async fn cancel_mcp_task_for_subject(
        &self,
        task_id: &str,
        subject: Option<&str>,
    ) -> Result<(), GatewayManagerError> {
        let route = self.resolve_task_route(task_id, subject)?;
        let pool = self.task_pool()?;
        cancel_native_task(&pool, &route).await?;
        Ok(())
    }

    fn task_pool(&self) -> Result<Arc<crate::upstream::pool::UpstreamPool>, GatewayManagerError> {
        self.ensure_ready()?;
        Ok(self.pool.read().expect("gateway pool poisoned").clone())
    }

    fn resolve_task_route(
        &self,
        task_id: &str,
        subject: Option<&str>,
    ) -> Result<TaskRoute, GatewayManagerError> {
        let route = self
            .task_routes
            .read()
            .expect("gateway task routes poisoned")
            .get(task_id)
            .cloned()
            .ok_or_else(|| GatewayManagerError::TaskMissing(task_id.to_owned()))?;
        if route.subject.as_deref() != subject {
            return Err(GatewayManagerError::TaskMissing(task_id.to_owned()));
        }
        Ok(route)
    }
}

async fn get_native_task(
    pool: &crate::upstream::pool::UpstreamPool,
    route: &TaskRoute,
) -> Result<Value, UpstreamError> {
    #[cfg(feature = "oauth")]
    if route.subject.is_some() {
        return pool
            .get_task_for_subject(
                &route.upstream,
                &route.native_task_id,
                route.subject.as_deref(),
            )
            .await;
    }
    pool.get_task(&route.upstream, &route.native_task_id).await
}

async fn update_native_task(
    pool: &crate::upstream::pool::UpstreamPool,
    route: &TaskRoute,
    input_responses: BTreeMap<String, Value>,
) -> Result<(), UpstreamError> {
    #[cfg(feature = "oauth")]
    if route.subject.is_some() {
        return pool
            .update_task_for_subject(
                &route.upstream,
                &route.native_task_id,
                input_responses,
                route.subject.as_deref(),
            )
            .await;
    }
    pool.update_task(&route.upstream, &route.native_task_id, input_responses)
        .await
}

async fn cancel_native_task(
    pool: &crate::upstream::pool::UpstreamPool,
    route: &TaskRoute,
) -> Result<(), UpstreamError> {
    #[cfg(feature = "oauth")]
    if route.subject.is_some() {
        return pool
            .cancel_task_for_subject(
                &route.upstream,
                &route.native_task_id,
                route.subject.as_deref(),
            )
            .await;
    }
    pool.cancel_task(&route.upstream, &route.native_task_id)
        .await
}

#[cfg(test)]
#[path = "mcp_tasks_tests.rs"]
mod tests;
