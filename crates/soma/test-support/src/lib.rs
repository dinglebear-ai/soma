// Test helper extraction point. Root crate keeps compatibility helpers for now.

pub mod tracing_capture;

pub use tracing_capture::{tracing_test_lock, SharedBuf, SharedWriter};

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use soma_application::{ApplicationPorts, SomaApplication};
use soma_client::SomaClient;
use soma_config::SomaConfig;
use soma_provider_core::ProviderCatalog;
use soma_service::{
    provider_registry::Provider, ProviderCall, ProviderError, ProviderOutput, ProviderRegistry,
    SomaService,
};

struct FixtureProvider {
    catalog: ProviderCatalog,
    output: Value,
}

#[async_trait]
impl Provider for FixtureProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, _call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        Ok(ProviderOutput::json(self.output.clone()))
    }
}

pub fn application_with_provider(catalog: ProviderCatalog, output: Value) -> Arc<SomaApplication> {
    let service = SomaService::new(
        SomaClient::new(&SomaConfig::default()).expect("test Soma client should build"),
    );
    let registry = ProviderRegistry::new(vec![Arc::new(FixtureProvider { catalog, output })])
        .expect("test provider registry should build");
    Arc::new(SomaApplication::new(
        Arc::new(service),
        Arc::new(registry),
        ApplicationPorts::unavailable(),
    ))
}

pub fn default_application() -> Arc<SomaApplication> {
    default_application_with_ports(ApplicationPorts::unavailable())
}

pub fn default_application_with_ports(ports: ApplicationPorts) -> Arc<SomaApplication> {
    let service = SomaService::new(
        SomaClient::new(&SomaConfig {
            api_url: String::new(),
            api_key: "test".into(),
            ..SomaConfig::default()
        })
        .expect("test Soma client should build"),
    );
    let registry = soma_service::static_provider_registry(service.clone())
        .expect("static test provider registry should build");
    Arc::new(SomaApplication::new(
        Arc::new(service),
        Arc::new(registry),
        ports,
    ))
}
