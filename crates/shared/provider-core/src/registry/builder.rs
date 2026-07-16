use std::{collections::BTreeMap, sync::Arc};

use crate::{
    Provider, ProviderId, ProviderRegistry, ProviderValidationError, validate_provider_manifest,
};

use super::RegistrySnapshot;

#[derive(Default)]
pub struct ProviderRegistryBuilder {
    providers: BTreeMap<String, Arc<dyn Provider>>,
}

impl ProviderRegistryBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        self,
        provider: impl Provider + 'static,
    ) -> Result<Self, ProviderValidationError> {
        self.register_arc(Arc::new(provider))
    }

    pub fn register_arc(
        mut self,
        provider: Arc<dyn Provider>,
    ) -> Result<Self, ProviderValidationError> {
        let catalog = provider.catalog();
        validate_provider_manifest(&catalog)?;
        let id = ProviderId::new(&catalog.provider.name).map_err(|error| {
            ProviderValidationError::new("invalid_provider_name", error.to_string())
        })?;
        if self.providers.insert(id.to_string(), provider).is_some() {
            return Err(ProviderValidationError::new(
                "duplicate_provider_name",
                format!("duplicate provider `{id}`"),
            ));
        }
        Ok(self)
    }

    pub fn build(self) -> Result<ProviderRegistry, ProviderValidationError> {
        let providers = Arc::new(self.providers);
        let catalogs = providers
            .values()
            .map(|provider| provider.catalog())
            .collect::<Vec<_>>();
        let snapshot = Arc::new(RegistrySnapshot::build(catalogs)?);
        Ok(ProviderRegistry {
            providers,
            snapshot,
        })
    }
}
