use std::fmt;

use sha2::{Digest, Sha256};

use crate::ProviderCatalog;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegistryFingerprint(String);

impl RegistryFingerprint {
    pub(super) fn from_catalogs(catalogs: &[ProviderCatalog]) -> Self {
        let canonical = serde_json::to_vec(catalogs).expect("provider catalogs serialize");
        let digest = Sha256::digest(canonical);
        let hex = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        Self(format!("sha256:{hex}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RegistryFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
