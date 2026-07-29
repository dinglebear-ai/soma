use async_trait::async_trait;

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
}
