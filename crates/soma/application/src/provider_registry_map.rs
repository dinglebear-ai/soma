use std::{collections::BTreeMap, sync::Arc};

use soma_domain::provider_validation::ProviderValidationError;

use super::Provider;

pub(super) fn provider_map(
    providers: Vec<Arc<dyn Provider>>,
) -> Result<BTreeMap<String, Arc<dyn Provider>>, ProviderValidationError> {
    let mut map = BTreeMap::new();
    for provider in providers {
        let name = provider.catalog().provider.name;
        if map.insert(name.clone(), provider).is_some() {
            return Err(ProviderValidationError::new(
                "duplicate_provider_name",
                format!("duplicate provider `{name}`"),
            ));
        }
    }
    Ok(map)
}
