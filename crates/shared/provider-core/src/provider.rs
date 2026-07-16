use async_trait::async_trait;

use crate::{ProviderCall, ProviderCatalog, ProviderError, ProviderOutput};

#[async_trait]
pub trait Provider: Send + Sync {
    fn catalog(&self) -> ProviderCatalog;

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError>;
}
