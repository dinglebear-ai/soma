//! Frozen pre-extraction provider source. Keep the public struct literals and
//! exhaustive surface match unchanged so this test remains a compile fixture.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use soma_contracts::providers::{
    McpOverlay, ProviderCatalog, ProviderIdentity, ProviderKind, ProviderManifest, ProviderPrompt,
    ProviderTool,
};
use soma_service::{
    provider_registry::{
        Provider, ProviderAuthMode, ProviderCall, ProviderOutput, ProviderPrincipal,
        ProviderRegistry, ProviderRequestLimits, ProviderSurface,
    },
    ProviderError,
};

struct PreExtractionProvider;

fn surface_name(surface: ProviderSurface) -> &'static str {
    match surface {
        ProviderSurface::Mcp => "mcp",
        ProviderSurface::Rest => "rest",
        ProviderSurface::Cli => "cli",
        ProviderSurface::Palette => "palette",
    }
}

#[async_trait]
impl Provider for PreExtractionProvider {
    fn catalog(&self) -> ProviderCatalog {
        ProviderManifest {
            schema_version: 1,
            provider: ProviderIdentity {
                name: "pre-extraction-provider".to_owned(),
                kind: ProviderKind::StaticRust,
                title: None,
                description: None,
                homepage: None,
                source: None,
                version: None,
                enabled: Some(true),
            },
            tools: vec![ProviderTool {
                name: "legacy_echo".to_owned(),
                description: "Legacy echo".to_owned(),
                title: None,
                input_schema: json!({"type": "object"}),
                output_schema: None,
                scope: None,
                destructive: false,
                requires_admin: false,
                cost: None,
                env: Vec::new(),
                limits: None,
                mcp: Some(McpOverlay {
                    enabled: true,
                    title: None,
                    annotations: json!({}),
                }),
                rest: None,
                cli: None,
                palette: None,
                ui: None,
                examples: Vec::new(),
                meta: json!({}),
            }],
            prompts: vec![ProviderPrompt {
                name: "legacy_prompt".to_owned(),
                description: "Legacy prompt".to_owned(),
                template: Some("Hello".to_owned()),
                arguments_schema: None,
                scope: None,
                mcp: Some(McpOverlay {
                    enabled: true,
                    title: None,
                    annotations: json!({}),
                }),
                examples: Vec::new(),
            }],
            resources: Vec::new(),
            tasks: Vec::new(),
            elicitation: Vec::new(),
            env: Vec::new(),
            capabilities: Default::default(),
            docs: None,
            plugin: None,
            ui: None,
            meta: json!({}),
        }
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        Ok(ProviderOutput::json(json!({
            "surface": surface_name(call.surface),
            "params": call.params,
        })))
    }
}

#[tokio::test]
async fn unchanged_pre_extraction_provider_source_compiles_and_dispatches() {
    let registry = ProviderRegistry::new(vec![Arc::new(PreExtractionProvider)]).unwrap();
    let output = registry
        .dispatch(ProviderCall {
            provider: String::new(),
            action: "legacy_echo".to_owned(),
            params: json!({}),
            principal: ProviderPrincipal::loopback_dev(),
            auth_mode: ProviderAuthMode::LoopbackDev,
            surface: ProviderSurface::Mcp,
            destructive_confirmed: false,
            limits: ProviderRequestLimits::default(),
            snapshot_id: String::new(),
        })
        .await
        .unwrap();

    assert_eq!(output.value["surface"], "mcp");
}
