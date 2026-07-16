use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use soma_contracts::providers::{
    CliOverlay, EnvRequirement, McpOverlay, ProviderCatalog, ProviderIdentity, ProviderKind,
    ProviderManifest, ProviderPrompt, ProviderResource, ProviderTool, RestOverlay,
};

use crate::{
    provider_errors::ProviderError,
    provider_registry::{Provider, ProviderCall, ProviderOutput},
    SomaService,
};

#[derive(Clone)]
pub struct RemoteCatalogProvider {
    service: SomaService,
    catalog: ProviderCatalog,
}

impl RemoteCatalogProvider {
    pub fn new(service: SomaService, catalog: ProviderCatalog) -> Self {
        Self { service, catalog }
    }
}

#[async_trait]
impl Provider for RemoteCatalogProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        self.service
            .call_rest_action(&call.action, call.params)
            .await
            .map(ProviderOutput::json)
            .map_err(|error| ProviderError::opaque_execution(&call.provider, call.action, error))
    }
}

pub fn catalogs_from_inspection(report: &Value) -> Result<Vec<ProviderCatalog>> {
    let providers = report
        .get("providers")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("remote provider catalog missing providers array"))?;
    providers.iter().map(catalog_from_provider).collect()
}

fn catalog_from_provider(provider: &Value) -> Result<ProviderCatalog> {
    let name = string_field(provider, "name")?;
    let kind = provider_kind(string_field(provider, "kind")?)?;
    Ok(ProviderManifest {
        schema_version: 1,
        provider: ProviderIdentity {
            name: name.to_owned(),
            kind,
            title: optional_string(provider, "title"),
            description: optional_string(provider, "description"),
            homepage: optional_string(provider, "homepage"),
            source: optional_string(provider, "source"),
            version: optional_string(provider, "version"),
            enabled: provider.get("enabled").and_then(Value::as_bool),
        },
        tools: provider
            .get("tools")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(tool_from_value)
            .collect::<Result<Vec<_>>>()?,
        prompts: provider
            .get("prompts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(prompt_from_value)
            .collect::<Result<Vec<_>>>()?,
        resources: provider
            .get("resources")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(resource_from_value)
            .collect::<Result<Vec<_>>>()?,
        tasks: Vec::new(),
        elicitation: Vec::new(),
        env: Vec::new(),
        capabilities: provider
            .get("declared_capabilities")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .context("remote provider declared_capabilities invalid")?
            .unwrap_or_default(),
        docs: None,
        plugin: None,
        ui: None,
        meta: json!({ "remote_catalog": true }),
    })
}

fn tool_from_value(tool: &Value) -> Result<ProviderTool> {
    Ok(ProviderTool {
        name: string_field(tool, "name")?.to_owned(),
        description: optional_string(tool, "description").unwrap_or_default(),
        title: optional_string(tool, "title"),
        input_schema: tool
            .get("input_schema")
            .cloned()
            .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
        output_schema: tool
            .get("output_schema")
            .cloned()
            .filter(|value| !value.is_null()),
        scope: optional_string(tool, "scope"),
        destructive: tool
            .get("destructive")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        requires_admin: tool
            .get("requires_admin")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        cost: optional_string(tool, "cost"),
        env: tool
            .get("env")
            .cloned()
            .map(serde_json::from_value::<Vec<EnvRequirement>>)
            .transpose()
            .context("remote tool env invalid")?
            .unwrap_or_default(),
        limits: tool
            .get("limits")
            .cloned()
            .filter(|value| !value.is_null())
            .map(serde_json::from_value)
            .transpose()
            .context("remote tool limits invalid")?,
        mcp: mcp_overlay(tool),
        rest: rest_overlay(tool)?,
        cli: tool
            .get("cli")
            .cloned()
            .filter(|value| !value.is_null())
            .map(serde_json::from_value::<CliOverlay>)
            .transpose()
            .context("remote tool cli overlay invalid")?,
        palette: None,
        ui: None,
        examples: Vec::new(),
        meta: json!({ "remote_catalog": true }),
    })
}

fn prompt_from_value(prompt: &Value) -> Result<ProviderPrompt> {
    Ok(ProviderPrompt {
        name: string_field(prompt, "name")?.to_owned(),
        title: optional_string(prompt, "title"),
        description: optional_string(prompt, "description").unwrap_or_default(),
        template: optional_string(prompt, "template"),
        arguments_schema: prompt
            .get("arguments_schema")
            .cloned()
            .filter(|value| !value.is_null()),
        scope: optional_string(prompt, "scope"),
        mcp: mcp_overlay(prompt),
        examples: Vec::new(),
    })
}

fn resource_from_value(resource: &Value) -> Result<ProviderResource> {
    Ok(ProviderResource {
        uri_template: string_field(resource, "uri_template")?.to_owned(),
        name: string_field(resource, "name")?.to_owned(),
        description: optional_string(resource, "description").unwrap_or_default(),
        mime_type: optional_string(resource, "mime_type"),
        scope: optional_string(resource, "scope"),
        mcp: mcp_overlay(resource),
        annotations: resource
            .get("annotations")
            .cloned()
            .unwrap_or_else(|| json!({})),
    })
}

fn mcp_overlay(value: &Value) -> Option<McpOverlay> {
    let enabled = value
        .get("surfaces")
        .and_then(|surfaces| surfaces.get("mcp"))
        .and_then(Value::as_bool)?;
    Some(McpOverlay {
        enabled,
        title: None,
        icons: Vec::new(),
        annotations: json!({}),
    })
}

fn rest_overlay(tool: &Value) -> Result<Option<RestOverlay>> {
    if let Some(rest) = tool.get("rest").filter(|value| !value.is_null()) {
        return serde_json::from_value(rest.clone())
            .map(Some)
            .context("remote tool rest overlay invalid");
    }
    if tool
        .get("surfaces")
        .and_then(|surfaces| surfaces.get("rest"))
        .and_then(Value::as_bool)
        == Some(false)
    {
        return Ok(Some(RestOverlay {
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
        }));
    }
    Ok(None)
}

fn provider_kind(kind: &str) -> Result<ProviderKind> {
    match kind {
        "static-rust" => Ok(ProviderKind::StaticRust),
        "openapi" => Ok(ProviderKind::Openapi),
        "ai-sdk" => Ok(ProviderKind::AiSdk),
        "wasm" => Ok(ProviderKind::Wasm),
        "mcp" => Ok(ProviderKind::Mcp),
        "python" => Ok(ProviderKind::Python),
        "langchain" => Ok(ProviderKind::Langchain),
        "llamaindex" => Ok(ProviderKind::Llamaindex),
        other => Err(anyhow!("unknown remote provider kind `{other}`")),
    }
}

fn string_field<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("remote catalog entry missing string field `{field}`"))
}

fn optional_string(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

#[cfg(test)]
#[path = "remote_tests.rs"]
mod tests;
