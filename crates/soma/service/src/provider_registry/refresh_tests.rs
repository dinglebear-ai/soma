use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use soma_contracts::providers::{
    CliOverlay, HostCapabilities, McpOverlay, ProviderCatalog, ProviderIdentity, ProviderKind,
    ProviderManifest, ProviderTool, RestOverlay,
};

use crate::{
    provider_errors::ProviderError,
    provider_registry::{Provider, ProviderCall, ProviderOutput, ProviderRegistry},
};

use super::ProviderRefreshEvent;

#[test]
fn refresh_event_reports_action_diff_and_enabled_surfaces() {
    let previous = snapshot(vec![tool("kept"), tool("removed")]);
    let next = snapshot(vec![
        tool("kept"),
        tool("added"),
        cli_only_tool("cli_only"),
        rest_tool("rest_action", "GET", "/v1/rest-action"),
    ]);

    let event = ProviderRefreshEvent::new(&previous, &next);

    assert_eq!(
        event.added_actions,
        vec!["added", "cli_only", "rest_action"]
    );
    assert_eq!(event.removed_actions, vec!["removed"]);
    assert_eq!(event.cli_actions, vec!["cli_only"]);
    assert_eq!(event.rest_routes, vec!["GET /v1/rest-action"]);
    assert_eq!(event.mcp_actions, vec!["added", "kept", "rest_action"]);
}

fn snapshot(tools: Vec<ProviderTool>) -> Arc<crate::provider_registry::RegistrySnapshot> {
    ProviderRegistry::new(vec![Arc::new(CatalogProvider(catalog(tools)))])
        .expect("registry")
        .snapshot()
}

#[derive(Clone)]
struct CatalogProvider(ProviderCatalog);

#[async_trait]
impl Provider for CatalogProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.0.clone()
    }

    async fn call(&self, _call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        Ok(ProviderOutput::json(json!({})))
    }
}

fn catalog(tools: Vec<ProviderTool>) -> ProviderCatalog {
    ProviderManifest {
        schema_version: 1,
        provider: ProviderIdentity {
            name: "test-provider".to_owned(),
            kind: ProviderKind::StaticRust,
            title: None,
            description: None,
            homepage: None,
            source: None,
            version: None,
            enabled: Some(true),
        },
        tools,
        prompts: Vec::new(),
        resources: Vec::new(),
        tasks: Vec::new(),
        elicitation: Vec::new(),
        env: Vec::new(),
        capabilities: HostCapabilities::default(),
        docs: None,
        plugin: None,
        ui: None,
        meta: json!({}),
    }
}

fn tool(name: &str) -> ProviderTool {
    ProviderTool {
        name: name.to_owned(),
        description: format!("{name} tool"),
        title: None,
        input_schema: json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {}
        }),
        output_schema: None,
        scope: Some("soma:read".to_owned()),
        destructive: false,
        requires_admin: false,
        cost: Some("cheap".to_owned()),
        env: Vec::new(),
        limits: None,
        mcp: None,
        rest: None,
        cli: None,
        palette: None,
        ui: None,
        examples: Vec::new(),
        meta: json!({}),
    }
}

fn cli_only_tool(name: &str) -> ProviderTool {
    ProviderTool {
        mcp: Some(McpOverlay {
            enabled: false,
            title: None,
            annotations: json!({}),
        }),
        cli: Some(CliOverlay {
            enabled: true,
            command: Some(name.to_owned()),
            aliases: Vec::new(),
            about: None,
            long_about: None,
            hidden: false,
            flags: Vec::new(),
            default_output: None,
            interactive: false,
        }),
        ..tool(name)
    }
}

fn rest_tool(name: &str, method: &str, path: &str) -> ProviderTool {
    ProviderTool {
        rest: Some(RestOverlay {
            enabled: true,
            method: Some(method.to_owned()),
            path: Some(path.to_owned()),
            tags: Vec::new(),
            summary: None,
            description: None,
            deprecated: false,
            path_params: json!({}),
            query_params: json!({}),
            request_body_schema: None,
        }),
        ..tool(name)
    }
}
