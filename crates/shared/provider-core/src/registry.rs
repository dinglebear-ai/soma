mod builder;
mod dispatch;
mod fingerprint;
mod index;
mod snapshot;

use std::{collections::BTreeMap, sync::Arc};

use crate::Provider;

pub use builder::ProviderRegistryBuilder;
pub use fingerprint::RegistryFingerprint;
pub use index::{ProviderIndexes, RegisteredTool};
pub use snapshot::RegistrySnapshot;

#[derive(Clone)]
pub struct ProviderRegistry {
    pub(super) providers: Arc<BTreeMap<String, Arc<dyn Provider>>>,
    pub(super) snapshot: Arc<RegistrySnapshot>,
}

impl ProviderRegistry {
    pub fn builder() -> ProviderRegistryBuilder {
        ProviderRegistryBuilder::new()
    }

    pub fn snapshot(&self) -> Arc<RegistrySnapshot> {
        Arc::clone(&self.snapshot)
    }
}
