use std::sync::Arc;

use crate::config::GatewayConfig;

use super::manager::{GatewayManager, GatewayManagerError};

#[derive(Clone)]
pub struct GatewayRuntime {
    manager: Arc<GatewayManager>,
}

impl GatewayRuntime {
    pub fn new(config: GatewayConfig) -> Result<Self, GatewayManagerError> {
        Ok(Self {
            manager: Arc::new(GatewayManager::new(config)?),
        })
    }

    #[must_use]
    pub fn manager(&self) -> Arc<GatewayManager> {
        Arc::clone(&self.manager)
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
