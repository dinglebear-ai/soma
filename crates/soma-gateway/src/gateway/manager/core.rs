use crate::config::GatewayConfig;

use super::{GatewayLifecycle, GatewayManager, GatewayManagerError};

impl GatewayManager {
    pub fn reload(&self, next: GatewayConfig) -> Result<(), GatewayManagerError> {
        {
            let mut lifecycle = self.lifecycle.write().expect("gateway lifecycle poisoned");
            *lifecycle = GatewayLifecycle::Reloading;
        }
        let result = self.replace_config_and_pool(next);
        *self.lifecycle.write().expect("gateway lifecycle poisoned") = GatewayLifecycle::Ready;
        result
    }

    fn replace_config_and_pool(&self, next: GatewayConfig) -> Result<(), GatewayManagerError> {
        next.validate()?;
        let next_pool = super::pool_lifecycle::build_pool_from_config(&next)?;
        *self.config.write().expect("gateway config poisoned") = next;
        *self.pool.write().expect("gateway pool poisoned") = std::sync::Arc::new(next_pool);
        Ok(())
    }
}

#[cfg(test)]
#[path = "core_tests.rs"]
mod tests;
