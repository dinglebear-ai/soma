use crate::{ProviderCatalog, ProviderValidationError};

use super::{ProviderIndexes, RegisteredTool, RegistryFingerprint};

#[derive(Clone)]
pub struct RegistrySnapshot {
    fingerprint: RegistryFingerprint,
    catalogs: Vec<ProviderCatalog>,
    indexes: ProviderIndexes,
}

impl RegistrySnapshot {
    pub(super) fn build(
        mut catalogs: Vec<ProviderCatalog>,
    ) -> Result<Self, ProviderValidationError> {
        catalogs.sort_by(|left, right| left.provider.name.cmp(&right.provider.name));
        let indexes = ProviderIndexes::build(&catalogs)?;
        let fingerprint = RegistryFingerprint::from_catalogs(&catalogs);
        Ok(Self {
            fingerprint,
            catalogs,
            indexes,
        })
    }

    pub fn fingerprint(&self) -> &RegistryFingerprint {
        &self.fingerprint
    }

    pub fn provider_count(&self) -> usize {
        self.catalogs.len()
    }

    pub fn catalogs(&self) -> &[ProviderCatalog] {
        &self.catalogs
    }

    pub fn tool(&self, action: &str) -> Option<&RegisteredTool> {
        self.indexes.tool(action)
    }

    pub fn action_names(&self) -> impl Iterator<Item = &str> {
        self.indexes.action_names()
    }

    pub fn route_action(&self, method: &str, path: &str) -> Option<&str> {
        self.indexes.route_action(method, path)
    }

    pub fn cli_action(&self, command: &str) -> Option<&str> {
        self.indexes.cli_action(command)
    }

    pub fn primitive_kind(&self, name: &str) -> Option<&str> {
        self.indexes.primitive_kind(name)
    }

    pub fn rest_routes(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.indexes.rest_routes()
    }

    pub fn compiled_validator_count(&self) -> usize {
        self.indexes.compiled_validator_count()
    }
}
