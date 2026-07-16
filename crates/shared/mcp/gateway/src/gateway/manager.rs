use std::sync::{Arc, RwLock};

use serde_json::Value;
use thiserror::Error;

use crate::config::{ConfigError, GatewayConfig, GatewayConfigView, UpstreamConfig};
use crate::gateway::config_store::FsGatewayConfigStore;
use crate::upstream::pool::{ToolCall, UpstreamPool};
use crate::upstream::{UpstreamError, UpstreamSnapshot};
use crate::usage::{NoopUsageSink, UsageEvent, UsageSink};

pub mod core;
pub mod mcp_routes;
#[cfg(feature = "protected-routes")]
pub mod mcp_scoped_routes;
#[cfg(feature = "oauth")]
pub mod oauth_lifecycle;
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
    #[error("gateway config store is not mounted")]
    StoreNotMounted,
    #[error("upstream `{0}` is already configured")]
    UpstreamExists(String),
    #[error("upstream `{0}` is not configured")]
    UpstreamMissing(String),
    #[error("gateway oauth runtime error: {0}")]
    OAuth(String),
}

impl From<soma_mcp_client::ConfigError> for GatewayManagerError {
    fn from(error: soma_mcp_client::ConfigError) -> Self {
        Self::Config(error.into())
    }
}

pub struct GatewayManager {
    config: RwLock<GatewayConfig>,
    pool: RwLock<Arc<UpstreamPool>>,
    lifecycle: RwLock<GatewayLifecycle>,
    usage: Arc<dyn UsageSink>,
    store: Option<FsGatewayConfigStore>,
    #[cfg(feature = "oauth")]
    oauth_runtime: RwLock<Option<Arc<soma_mcp_client::oauth::UpstreamOAuthRuntime>>>,
}

impl GatewayManager {
    pub fn new(config: GatewayConfig) -> Result<Self, GatewayManagerError> {
        Self::with_usage(config, Arc::new(NoopUsageSink))
    }

    pub fn with_usage(
        config: GatewayConfig,
        usage: Arc<dyn UsageSink>,
    ) -> Result<Self, GatewayManagerError> {
        Self::build(config, usage, None)
    }

    pub fn from_store(store: FsGatewayConfigStore) -> Result<Self, GatewayManagerError> {
        let config = store.load_or_install_default()?;
        Self::build(config, Arc::new(NoopUsageSink), Some(store))
    }

    fn build(
        config: GatewayConfig,
        usage: Arc<dyn UsageSink>,
        store: Option<FsGatewayConfigStore>,
    ) -> Result<Self, GatewayManagerError> {
        config.validate()?;
        let pool = pool_lifecycle::build_pool_from_config(&config)?;
        Ok(Self {
            config: RwLock::new(config),
            pool: RwLock::new(Arc::new(pool)),
            lifecycle: RwLock::new(GatewayLifecycle::Ready),
            usage,
            store,
            #[cfg(feature = "oauth")]
            oauth_runtime: RwLock::new(None),
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

    pub async fn discover(&self) -> Result<Vec<UpstreamSnapshot>, GatewayManagerError> {
        self.ensure_ready()?;
        let pool = self.pool.read().expect("gateway pool poisoned").clone();
        Ok(pool.discover().await?)
    }

    pub fn exposed_tool_count(&self) -> Result<usize, GatewayManagerError> {
        self.ensure_ready()?;
        Ok(self
            .pool
            .read()
            .expect("gateway pool poisoned")
            .exposed_tool_count())
    }

    pub async fn call_tool(
        &self,
        upstream: impl Into<String>,
        tool: impl Into<String>,
        params: Value,
    ) -> Result<Value, GatewayManagerError> {
        self.ensure_ready()?;
        let upstream = upstream.into();
        let tool = tool.into();
        let pool = self.pool.read().expect("gateway pool poisoned").clone();
        let result = pool
            .call_tool(ToolCall {
                upstream: upstream.clone(),
                tool,
                params,
            })
            .await;
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

    pub fn add_upstream(
        &self,
        upstream: UpstreamConfig,
    ) -> Result<GatewayConfigView, GatewayManagerError> {
        upstream.validate()?;
        self.mutate_config(|config| {
            if config
                .upstream
                .iter()
                .any(|item| item.name == upstream.name)
            {
                return Err(GatewayManagerError::UpstreamExists(upstream.name.clone()));
            }
            config.upstream.push(upstream);
            Ok(())
        })
    }

    pub fn update_upstream(
        &self,
        upstream: UpstreamConfig,
    ) -> Result<GatewayConfigView, GatewayManagerError> {
        upstream.validate()?;
        self.mutate_config(|config| {
            let Some(slot) = config
                .upstream
                .iter_mut()
                .find(|item| item.name == upstream.name)
            else {
                return Err(GatewayManagerError::UpstreamMissing(upstream.name.clone()));
            };
            *slot = upstream;
            Ok(())
        })
    }

    pub fn remove_upstream(&self, name: &str) -> Result<GatewayConfigView, GatewayManagerError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(GatewayManagerError::Config(ConfigError::invalid(
                "name",
                "must not be empty",
            )));
        }
        self.mutate_config(|config| {
            let before = config.upstream.len();
            config.upstream.retain(|item| item.name != name);
            if config.upstream.len() == before {
                return Err(GatewayManagerError::UpstreamMissing(name.to_owned()));
            }
            Ok(())
        })
    }

    pub fn reload_from_store(&self) -> Result<GatewayConfigView, GatewayManagerError> {
        let Some(store) = &self.store else {
            return Err(GatewayManagerError::StoreNotMounted);
        };
        self.replace_config(store.load()?)
    }

    fn ensure_ready(&self) -> Result<(), GatewayManagerError> {
        if self.lifecycle() == GatewayLifecycle::Ready {
            return Ok(());
        }
        Err(GatewayManagerError::GatewayReloading)
    }

    fn mutate_config(
        &self,
        mutate: impl FnOnce(&mut GatewayConfig) -> Result<(), GatewayManagerError>,
    ) -> Result<GatewayConfigView, GatewayManagerError> {
        let mut next = self.config.read().expect("gateway config poisoned").clone();
        mutate(&mut next)?;
        if let Some(store) = &self.store {
            store.save(&next)?;
        }
        self.replace_config(next)
    }

    fn replace_config(
        &self,
        next: GatewayConfig,
    ) -> Result<GatewayConfigView, GatewayManagerError> {
        next.validate()?;
        self.with_reloading(|| {
            self.replace_config_and_pool(next)?;
            Ok(self.config_view())
        })
    }

    pub(super) fn reload_config(&self, next: GatewayConfig) -> Result<(), GatewayManagerError> {
        next.validate()?;
        self.with_reloading(|| self.replace_config_and_pool(next))
    }

    fn replace_config_and_pool(&self, next: GatewayConfig) -> Result<(), GatewayManagerError> {
        let pool = pool_lifecycle::build_pool_from_config(&next)?;
        #[cfg(feature = "oauth")]
        if let Some(runtime) = self
            .oauth_runtime
            .read()
            .expect("gateway oauth runtime poisoned")
            .as_ref()
        {
            pool.install_oauth_provider(runtime.provider());
        }
        *self.config.write().expect("gateway config poisoned") = next;
        *self.pool.write().expect("gateway pool poisoned") = Arc::new(pool);
        Ok(())
    }

    #[cfg(feature = "oauth")]
    pub fn install_upstream_oauth_runtime(
        &self,
        runtime: soma_mcp_client::oauth::UpstreamOAuthRuntime,
    ) {
        let runtime = Arc::new(runtime);
        self.pool
            .read()
            .expect("gateway pool poisoned")
            .install_oauth_provider(runtime.provider());
        *self
            .oauth_runtime
            .write()
            .expect("gateway oauth runtime poisoned") = Some(runtime);
    }

    fn with_reloading<T>(
        &self,
        operation: impl FnOnce() -> Result<T, GatewayManagerError>,
    ) -> Result<T, GatewayManagerError> {
        *self.lifecycle.write().expect("gateway lifecycle poisoned") = GatewayLifecycle::Reloading;
        let result = operation();
        *self.lifecycle.write().expect("gateway lifecycle poisoned") = GatewayLifecycle::Ready;
        result
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
