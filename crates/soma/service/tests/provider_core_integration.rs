use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use soma_contracts::providers::{ProviderCatalog, ProviderManifest, ToolSpec};
use soma_provider_core::ProviderId;
use soma_service::provider_registry::{
    Provider, ProviderAuthMode, ProviderCall, ProviderOutput, ProviderPrincipal, ProviderRegistry,
    ProviderRequestLimits, ProviderSurface,
};
use soma_service::ProviderError;

struct LegacyShapeProvider {
    catalog: ProviderCatalog,
    calls: Mutex<Vec<ProviderCall>>,
}

impl LegacyShapeProvider {
    fn new() -> Self {
        Self {
            catalog: ProviderManifest::new(
                ProviderId::new("legacy-shape").unwrap(),
                "Legacy shape",
                "1.0.0",
            )
            .with_tool(ToolSpec::new("echo", "Echo", json!({"type": "object"}))),
            calls: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl Provider for LegacyShapeProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        let output = json!({
            "provider": call.provider,
            "subject": call.principal.subject,
            "snapshot_id": call.snapshot_id,
        });
        self.calls.lock().unwrap().push(call);
        Ok(ProviderOutput::json(output))
    }
}

#[tokio::test]
async fn soma_preserves_legacy_provider_shape_and_uses_core_registry_snapshot() {
    let provider = Arc::new(LegacyShapeProvider::new());
    let registry = ProviderRegistry::new(vec![provider.clone()]).unwrap();
    let snapshot = registry.snapshot();

    assert_eq!(snapshot.core_snapshot().provider_count(), 1);
    assert_eq!(
        snapshot.fingerprint,
        snapshot.core_snapshot().fingerprint().as_str()
    );
    assert_eq!(
        snapshot.action_names(),
        snapshot.core_snapshot().action_names().collect::<Vec<_>>()
    );

    let output = registry
        .dispatch(ProviderCall {
            provider: String::new(),
            action: "echo".to_owned(),
            params: json!({}),
            principal: ProviderPrincipal {
                subject: "compat-user".to_owned(),
                scopes: Vec::new(),
            },
            auth_mode: ProviderAuthMode::LoopbackDev,
            surface: ProviderSurface::Internal,
            destructive_confirmed: false,
            limits: ProviderRequestLimits::default(),
            snapshot_id: String::new(),
        })
        .await
        .unwrap();

    assert_eq!(output.value["provider"], "legacy-shape");
    assert_eq!(output.value["subject"], "compat-user");
    assert_eq!(provider.calls.lock().unwrap().len(), 1);
}
