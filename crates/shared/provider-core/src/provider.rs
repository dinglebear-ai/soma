use async_trait::async_trait;
use serde_json::Value;

use crate::{ProviderCall, ProviderCatalog, ProviderError, ProviderOutput};

#[async_trait]
pub trait Provider: Send + Sync {
    fn catalog(&self) -> ProviderCatalog;

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError>;

    /// Stop accepting new work and release provider-owned runtime resources.
    ///
    /// Stateless providers use the default no-op. Registries call this after
    /// an atomic generation swap, outside registry locks.
    async fn retire(&self) {}

    /// Returns product-neutral operator status for a stateful runtime.
    fn runtime_status(&self) -> Option<Value> {
        None
    }

    /// Cancels active runtime work without waiting for the provider call lock.
    fn cancel_active(&self) -> bool {
        false
    }

    /// Clears runtime quarantine after explicit operator authorization.
    async fn reset_quarantine(&self) {}

    /// Stops new work on a retained generation while in-flight work drains.
    fn deactivate(&self) {}

    /// Re-enables a retained generation selected by operator rollback.
    fn activate(&self) {}
}
