//! Bounded OpenAPI provider skeleton.
//!
//! The curated importer and SSRF-safe executor are intentionally narrow in this
//! slice: manifests validate and registry policy can carry network grants, but
//! arbitrary upstream execution is deferred until the OpenAPI operation tests
//! are implemented around host/scheme/path pinning.

use std::sync::Arc;

use async_trait::async_trait;
use rtemplate_contracts::providers::ProviderCatalog;

use crate::{
    provider_errors::ProviderError,
    provider_registry::{Provider, ProviderCall, ProviderOutput},
};

#[derive(Clone)]
pub struct OpenApiProvider {
    catalog: ProviderCatalog,
}

impl OpenApiProvider {
    pub fn curated(catalog: ProviderCatalog) -> Self {
        Self { catalog }
    }

    pub fn arc(catalog: ProviderCatalog) -> Arc<Self> {
        Arc::new(Self::curated(catalog))
    }
}

#[async_trait]
impl Provider for OpenApiProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        Err(ProviderError::new(
            "openapi_provider_execution_deferred",
            call.provider,
            Some(call.action),
            "OpenAPIProvider execution is blocked until SSRF-safe operation tests land",
            "Implement curated operation execution with scheme/host/port/path pinning before enabling this provider.",
        ))
    }
}
