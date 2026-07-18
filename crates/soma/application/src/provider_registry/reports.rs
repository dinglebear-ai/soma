use serde_json::{json, Value};

use super::{provider_tool_surface_enabled, ProviderSurface, RegistrySnapshot};

impl RegistrySnapshot {
    pub fn validation_summary(&self) -> Value {
        let actions = self.action_names();
        json!({
            "schema_version": 1,
            "ok": true,
            "provider_fingerprint": self.fingerprint.clone(),
            "provider_count": self.catalogs.len(),
            "action_count": actions.len(),
            "compiled_validator_count": self.compiled_validator_count,
            "actions": actions
        })
    }

    pub fn inspection_report(&self) -> Value {
        let providers = self
            .catalogs
            .iter()
            .map(|catalog| {
                let kind = catalog.provider.kind.as_str();
                let tools = catalog
                    .tools
                    .iter()
                    .map(|tool| {
                        let rest_enabled =
                            provider_tool_surface_enabled(tool, ProviderSurface::Rest);
                        let generic_rest = rest_enabled.then(|| {
                            json!({
                                "enabled": true,
                                "method": "POST",
                                "path": format!("/v1/tools/{}", tool.name),
                            })
                        });
                        json!({
                            "name": tool.name.clone(),
                            "title": tool.title.clone(),
                            "description": tool.description.clone(),
                            "input_schema": tool.input_schema.clone(),
                            "output_schema": tool.output_schema.clone(),
                            "scope": tool.scope.clone(),
                            "destructive": tool.destructive,
                            "requires_admin": tool.requires_admin,
                            "surfaces": {
                                "mcp": provider_tool_surface_enabled(tool, ProviderSurface::Mcp),
                                "rest": rest_enabled,
                                "cli": provider_tool_surface_enabled(tool, ProviderSurface::Cli),
                                "palette": provider_tool_surface_enabled(tool, ProviderSurface::Palette)
                            },
                            "rest": tool.rest.clone(),
                            "cli": tool.cli.clone(),
                            "generic_rest": generic_rest,
                            "limits": tool.limits.clone(),
                            "env": tool.env.clone(),
                        })
                    })
                    .collect::<Vec<_>>();
                let prompts = catalog
                    .prompts
                    .iter()
                    .map(|prompt| {
                        json!({
                            "name": prompt.name.clone(),
                            "description": prompt.description.clone(),
                            "template": prompt.template.clone(),
                            "arguments_schema": prompt.arguments_schema.clone(),
                            "scope": prompt.scope.clone(),
                            "surfaces": {
                                "mcp": prompt.mcp.as_ref().map(|mcp| mcp.enabled).unwrap_or(true)
                            },
                            "examples": prompt.examples.clone(),
                        })
                    })
                    .collect::<Vec<_>>();
                let resources = catalog
                    .resources
                    .iter()
                    .map(|resource| {
                        json!({
                            "name": resource.name.clone(),
                            "uri_template": resource.uri_template.clone(),
                            "description": resource.description.clone(),
                            "mime_type": resource.mime_type.clone(),
                            "scope": resource.scope.clone(),
                            "surfaces": {
                                "mcp": resource.mcp.as_ref().map(|mcp| mcp.enabled).unwrap_or(true)
                            },
                            "annotations": resource.annotations.clone(),
                        })
                    })
                    .collect::<Vec<_>>();
                json!({
                    "name": catalog.provider.name.clone(),
                    "kind": kind,
                    "title": catalog.provider.title.clone(),
                    "enabled": catalog.provider.enabled.unwrap_or(true),
                    "version": catalog.provider.version.clone(),
                    "source": catalog.provider.source.clone(),
                    "declared_capabilities": catalog.capabilities.clone(),
                    "runtime_security": provider_runtime_security(kind),
                    "tools": tools,
                    "prompts": prompts,
                    "resources": resources,
                })
            })
            .collect::<Vec<_>>();
        json!({
            "schema_version": 1,
            "provider_fingerprint": self.fingerprint.clone(),
            "compiled_validator_count": self.compiled_validator_count,
            "actions": self.action_names(),
            "providers": providers,
        })
    }
}

fn provider_runtime_security(kind: &str) -> Value {
    match kind {
        "wasm" => json!({
            "runtime": "wasmtime",
            "trust": "sandboxed",
            "capability_enforcement": "registry broker enforces declared host capabilities before dispatch"
        }),
        "ai-sdk" | "python" | "langchain" | "llamaindex" => json!({
            "runtime": "sidecar-process",
            "trust": "trusted-local-code",
            "capability_enforcement": "registry broker enforces declared host capabilities before dispatch"
        }),
        "openapi" | "mcp" => json!({
            "runtime": "remote-or-upstream",
            "trust": "upstream-service",
            "capability_enforcement": "registry broker enforces declared host capabilities before dispatch"
        }),
        _ => json!({
            "runtime": "in-process",
            "trust": "trusted-binary",
            "capability_enforcement": "registry broker enforces declared host capabilities before dispatch"
        }),
    }
}

#[cfg(test)]
#[path = "reports_tests.rs"]
mod tests;
