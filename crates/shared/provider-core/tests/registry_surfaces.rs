use async_trait::async_trait;
use serde_json::json;
use soma_provider_core::{
    CliOverlay, McpOverlay, PaletteOverlay, Provider, ProviderCall, ProviderCatalog, ProviderError,
    ProviderId, ProviderManifest, ProviderOutput, ProviderRegistry, ProviderSurface, RestOverlay,
    ToolSpec, UiOverlay,
};

#[derive(Clone)]
struct FakeProvider(ProviderCatalog);

#[async_trait]
impl Provider for FakeProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.0.clone()
    }

    async fn call(&self, _call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        Ok(ProviderOutput::value(json!({"ok": true})))
    }
}

fn registry(tool: ToolSpec) -> ProviderRegistry {
    ProviderRegistry::builder()
        .register(FakeProvider(
            ProviderManifest::new(
                ProviderId::new("surface-provider").unwrap(),
                "Surface provider",
                "1.0.0",
            )
            .with_tool(tool),
        ))
        .unwrap()
        .build()
        .unwrap()
}

fn disabled_tool() -> ToolSpec {
    let mut tool = ToolSpec::new("run", "run", json!({"type": "object"}));
    tool.mcp = Some(McpOverlay {
        enabled: false,
        title: None,
        annotations: json!({}),
    });
    tool.rest = Some(RestOverlay {
        enabled: false,
        method: None,
        path: None,
        tags: Vec::new(),
        summary: None,
        description: None,
        deprecated: false,
        path_params: json!({}),
        query_params: json!({}),
        request_body_schema: None,
    });
    tool.cli = Some(CliOverlay {
        enabled: false,
        command: None,
        aliases: Vec::new(),
        about: None,
        long_about: None,
        hidden: false,
        flags: Vec::new(),
        default_output: None,
        interactive: false,
    });
    tool.palette = Some(PaletteOverlay {
        enabled: false,
        category: None,
        icon: None,
        tone: None,
        arg_mode: None,
        result_view: None,
        aurora_blocks: Vec::new(),
    });
    tool.ui = Some(UiOverlay {
        enabled: false,
        aurora_registry_dependencies: Vec::new(),
        shadcn_items: Vec::new(),
        categories: Vec::new(),
        meta: json!({}),
    });
    tool
}

#[tokio::test]
async fn disabled_surfaces_are_rejected_and_internal_bypasses_exposure() {
    let registry = registry(disabled_tool());

    for surface in [
        ProviderSurface::Mcp,
        ProviderSurface::Rest,
        ProviderSurface::Cli,
        ProviderSurface::Palette,
        ProviderSurface::Ui,
    ] {
        let error = registry
            .dispatch(ProviderCall::new("run", json!({})).with_surface(surface))
            .await
            .expect_err("disabled surface must fail");
        assert_eq!(error.code.as_ref(), "surface_not_exposed");
    }

    registry
        .dispatch(ProviderCall::new("run", json!({})).with_surface(ProviderSurface::Internal))
        .await
        .expect("internal dispatch bypasses surface exposure");
}

#[tokio::test]
async fn absent_overlays_use_compatible_surface_defaults() {
    let registry = registry(ToolSpec::new("run", "run", json!({"type": "object"})));

    for surface in [
        ProviderSurface::Mcp,
        ProviderSurface::Rest,
        ProviderSurface::Palette,
        ProviderSurface::Ui,
    ] {
        registry
            .dispatch(ProviderCall::new("run", json!({})).with_surface(surface))
            .await
            .expect("surface defaults to exposed");
    }

    let cli = registry
        .dispatch(ProviderCall::new("run", json!({})).with_surface(ProviderSurface::Cli))
        .await
        .expect_err("CLI defaults to hidden");
    assert_eq!(cli.code.as_ref(), "surface_not_exposed");
}
