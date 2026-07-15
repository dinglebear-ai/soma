use std::sync::{Arc, RwLock};

use serde_json::Value;
use thiserror::Error;

use crate::config::{ConfigError, GatewayConfig, GatewayConfigView};
use crate::upstream::pool::{ToolCall, UpstreamPool};
use crate::upstream::{UpstreamError, UpstreamSnapshot};
use crate::usage::{NoopUsageSink, UsageEvent, UsageSink};

pub mod core;
pub mod pool_lifecycle;
#[cfg(feature = "protected-routes")]
pub mod protected_routes;
#[cfg(feature = "protected-routes")]
pub mod virtual_servers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayLifecycle {
    Ready,
    Reloading,
}

#[derive(Debug, Error)]
pub enum GatewayManagerError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Upstream(#[from] UpstreamError),
    #[error("gateway_reloading")]
    GatewayReloading,
}

pub struct GatewayManager {
    config: RwLock<GatewayConfig>,
    pool: RwLock<Arc<UpstreamPool>>,
    lifecycle: RwLock<GatewayLifecycle>,
    usage: Arc<dyn UsageSink>,
}

impl GatewayManager {
    pub fn new(config: GatewayConfig) -> Result<Self, GatewayManagerError> {
        Self::with_usage(config, Arc::new(NoopUsageSink))
    }

    pub fn with_usage(
        config: GatewayConfig,
        usage: Arc<dyn UsageSink>,
    ) -> Result<Self, GatewayManagerError> {
        config.validate()?;
        let pool = pool_lifecycle::build_pool_from_config(&config)?;
        Ok(Self {
            config: RwLock::new(config),
            pool: RwLock::new(Arc::new(pool)),
            lifecycle: RwLock::new(GatewayLifecycle::Ready),
            usage,
        })
    }

    #[must_use]
    pub fn lifecycle(&self) -> GatewayLifecycle {
        *self.lifecycle.read().expect("gateway lifecycle poisoned")
    }

    #[must_use]
    pub fn config_view(&self) -> GatewayConfigView {
        self.config
            .read()
            .expect("gateway config poisoned")
            .redacted_view()
    }

    pub fn discover(&self) -> Result<Vec<UpstreamSnapshot>, GatewayManagerError> {
        self.ensure_ready()?;
        Ok(self
            .pool
            .read()
            .expect("gateway pool poisoned")
            .discover()?)
    }

    pub fn call_tool(
        &self,
        upstream: impl Into<String>,
        tool: impl Into<String>,
        params: Value,
    ) -> Result<Value, GatewayManagerError> {
        self.ensure_ready()?;
        let upstream = upstream.into();
        let tool = tool.into();
        let result = self
            .pool
            .read()
            .expect("gateway pool poisoned")
            .call_tool(ToolCall {
                upstream: upstream.clone(),
                tool,
                params,
            });
        let success = result.is_ok();
        let bytes = result
            .as_ref()
            .ok()
            .and_then(|value| serde_json::to_vec(value).ok())
            .map_or(0, |bytes| bytes.len());
        self.usage.record(UsageEvent {
            action: "call_tool".to_owned(),
            upstream: Some(upstream),
            success,
            bytes,
        });
        Ok(result?)
    }

    fn ensure_ready(&self) -> Result<(), GatewayManagerError> {
        if self.lifecycle() == GatewayLifecycle::Ready {
            return Ok(());
        }
        Err(GatewayManagerError::GatewayReloading)
    }

    #[cfg(test)]
    pub(crate) fn install_pool_for_tests(&self, pool: UpstreamPool) {
        *self.pool.write().expect("gateway pool poisoned") = Arc::new(pool);
    }

    #[cfg(test)]
    pub(crate) fn set_lifecycle_for_tests(&self, lifecycle: GatewayLifecycle) {
        *self.lifecycle.write().expect("gateway lifecycle poisoned") = lifecycle;
    }
}

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;
