use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

use serde_json::Value;

#[cfg(feature = "protected-routes")]
use crate::gateway::protected_routes::ProtectedRouteScope;
use crate::upstream::{McpRequestOutcome, UpstreamError};

use super::{GatewayManager, GatewayManagerError, TaskRoute, TaskRouteScopeKey};

const TASK_ROUTE_IDLE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const TASK_ROUTE_MAX_ENTRIES: usize = 4096;

impl GatewayManager {
    pub(super) fn register_task_outcome(
        &self,
        outcome: McpRequestOutcome,
        upstream: &str,
        subject: Option<&str>,
    ) -> Result<McpRequestOutcome, GatewayManagerError> {
        self.register_task_outcome_with_scope_key(outcome, upstream, subject, None)
    }

    #[cfg(feature = "protected-routes")]
    pub(super) fn register_task_outcome_for_scope(
        &self,
        outcome: McpRequestOutcome,
        upstream: &str,
        subject: Option<&str>,
        scope: Option<&ProtectedRouteScope>,
    ) -> Result<McpRequestOutcome, GatewayManagerError> {
        self.register_task_outcome_with_scope_key(
            outcome,
            upstream,
            subject,
            scope.map(TaskRouteScopeKey::from_scope),
        )
    }

    fn register_task_outcome_with_scope_key(
        &self,
        outcome: McpRequestOutcome,
        upstream: &str,
        subject: Option<&str>,
        scope: Option<TaskRouteScopeKey>,
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
        let now = Instant::now();
        let mut routes = self
            .task_routes
            .write()
            .expect("gateway task routes poisoned");
        prepare_task_routes_for_insert(&mut routes, now, TASK_ROUTE_MAX_ENTRIES);
        routes.insert(
            public_task_id.clone(),
            TaskRoute {
                upstream: upstream.to_owned(),
                native_task_id,
                subject: subject.map(str::to_owned),
                scope,
                last_used: now,
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

    #[cfg(feature = "protected-routes")]
    pub async fn get_mcp_task_for_subject_and_scope(
        &self,
        task_id: &str,
        subject: Option<&str>,
        scope: Option<&ProtectedRouteScope>,
    ) -> Result<Value, GatewayManagerError> {
        let route = self.resolve_task_route_for_scope(task_id, subject, scope)?;
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

    #[cfg(feature = "protected-routes")]
    pub async fn update_mcp_task_for_subject_and_scope(
        &self,
        task_id: &str,
        input_responses: BTreeMap<String, Value>,
        subject: Option<&str>,
        scope: Option<&ProtectedRouteScope>,
    ) -> Result<(), GatewayManagerError> {
        let route = self.resolve_task_route_for_scope(task_id, subject, scope)?;
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

    #[cfg(feature = "protected-routes")]
    pub async fn cancel_mcp_task_for_subject_and_scope(
        &self,
        task_id: &str,
        subject: Option<&str>,
        scope: Option<&ProtectedRouteScope>,
    ) -> Result<(), GatewayManagerError> {
        let route = self.resolve_task_route_for_scope(task_id, subject, scope)?;
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
        self.resolve_task_route_for_scope_key(task_id, subject, None)
    }

    #[cfg(feature = "protected-routes")]
    fn resolve_task_route_for_scope(
        &self,
        task_id: &str,
        subject: Option<&str>,
        scope: Option<&ProtectedRouteScope>,
    ) -> Result<TaskRoute, GatewayManagerError> {
        self.resolve_task_route_for_scope_key(
            task_id,
            subject,
            scope.map(TaskRouteScopeKey::from_scope),
        )
    }

    fn resolve_task_route_for_scope_key(
        &self,
        task_id: &str,
        subject: Option<&str>,
        expected_scope: Option<TaskRouteScopeKey>,
    ) -> Result<TaskRoute, GatewayManagerError> {
        let now = Instant::now();
        let mut routes = self
            .task_routes
            .write()
            .expect("gateway task routes poisoned");
        prune_expired_task_routes(&mut routes, now);
        let route = routes
            .get_mut(task_id)
            .ok_or_else(|| GatewayManagerError::TaskMissing(task_id.to_owned()))?;
        if route.subject.as_deref() != subject || route.scope != expected_scope {
            return Err(GatewayManagerError::TaskMissing(task_id.to_owned()));
        }
        if route
            .scope
            .as_ref()
            .is_some_and(|scope| !scope.upstreams.contains(&route.upstream))
        {
            return Err(GatewayManagerError::TaskMissing(task_id.to_owned()));
        }
        route.last_used = now;
        Ok(route.clone())
    }
}

fn prune_expired_task_routes(routes: &mut HashMap<String, TaskRoute>, now: Instant) {
    routes.retain(|_, route| now.saturating_duration_since(route.last_used) <= TASK_ROUTE_IDLE_TTL);
}

fn prepare_task_routes_for_insert(
    routes: &mut HashMap<String, TaskRoute>,
    now: Instant,
    max_entries: usize,
) {
    prune_expired_task_routes(routes, now);
    if max_entries == 0 {
        routes.clear();
        return;
    }
    while routes.len() >= max_entries {
        let Some(oldest) = routes
            .iter()
            .min_by_key(|(_, route)| route.last_used)
            .map(|(task_id, _)| task_id.clone())
        else {
            break;
        };
        routes.remove(&oldest);
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
