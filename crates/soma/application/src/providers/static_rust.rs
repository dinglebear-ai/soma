use async_trait::async_trait;
use serde_json::{json, Map, Value};
use soma_domain::actions::{ActionTransport, SomaAction, ACTION_SPECS};
use soma_provider_core::{
    CliOverlay, DocsOverlay, McpOverlay, PaletteOverlay, ProviderCatalog, ProviderIdentity,
    ProviderKind, ProviderManifest, ProviderPrompt, ProviderTool, RestOverlay,
};

use crate::{
    dispatch_action,
    provider_errors::ProviderError,
    provider_registry::{Provider, ProviderCall, ProviderOutput},
    SomaService,
};

#[derive(Clone)]
pub struct StaticRustProvider {
    service: SomaService,
    catalog: ProviderCatalog,
}

impl StaticRustProvider {
    pub fn new(service: SomaService) -> Self {
        Self {
            service,
            catalog: static_catalog(),
        }
    }

    pub fn catalog_static() -> ProviderCatalog {
        static_catalog()
    }
}

#[async_trait]
impl Provider for StaticRustProvider {
    fn catalog(&self) -> ProviderCatalog {
        self.catalog.clone()
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        let action = SomaAction::from_rest(&call.action, &call.params)
            .or_else(|_| SomaAction::from_mcp_args(&action_params(&call.action, &call.params)))
            .map_err(|error| {
                ProviderError::validation(
                    "static-rust",
                    call.action.clone(),
                    "invalid_static_action_input",
                    error.to_string(),
                )
            })?;
        let value = match action {
            SomaAction::Help => crate::execute_service_action(&self.service, &action)
                .await
                .map_err(|error| ProviderError::execution("static-rust", call.action, error))?,
            SomaAction::ElicitName | SomaAction::ScaffoldIntent => {
                return Err(ProviderError::validation(
                    "static-rust",
                    call.action,
                    "mcp_peer_required",
                    "elicitation actions require a live MCP peer",
                ));
            }
            other => dispatch_action(&self.service, &other, surface_label(call.surface))
                .await
                .map_err(|error| ProviderError::execution("static-rust", call.action, error))?,
        };
        Ok(ProviderOutput::json(value))
    }
}

fn static_catalog() -> ProviderCatalog {
    ProviderManifest {
        schema_version: 1,
        provider: ProviderIdentity {
            name: "static-rust".to_owned(),
            kind: ProviderKind::StaticRust,
            title: Some("Built-in Rust actions".to_owned()),
            description: Some("Native service actions compiled into Soma.".to_owned()),
            homepage: None,
            source: None,
            version: None,
            enabled: Some(true),
        },
        tools: ACTION_SPECS.iter().map(static_tool).collect(),
        // Reserves `quick_start` in the directory-wide uniqueness namespace
        // (`filesystem_uniqueness::apply_directory_wide_checks` and
        // `provider_registry`'s own duplicate-primitive check both seed from
        // this catalog) so a drop-in provider can't declare a same-named
        // prompt and silently shadow the built-in one. Content lives in
        // `soma_mcp::prompts::list_prompts`/`get_prompt` — this entry exists
        // for name reservation only, not to serve as the source of truth.
        prompts: vec![ProviderPrompt {
            name: "quick_start".to_owned(),
            description: "Check the server status and get a personalised greeting to verify \
                the MCP connection is working end-to-end."
                .to_owned(),
            template: None,
            arguments_schema: None,
            scope: None,
            mcp: None,
            examples: Vec::new(),
        }],
        resources: Vec::new(),
        tasks: Vec::new(),
        elicitation: Vec::new(),
        env: Vec::new(),
        capabilities: Default::default(),
        docs: Some(DocsOverlay {
            when_to_use: Some(
                "Use for Soma built-in Rust actions, scaffold intent collection, MCP elicitation flows, and CLI/REST action reference."
                    .to_owned(),
            ),
            examples: Vec::new(),
            troubleshooting: Vec::new(),
        }),
        plugin: None,
        ui: None,
        meta: json!({}),
    }
}

fn static_tool(spec: &soma_domain::actions::ActionSpec) -> ProviderTool {
    ProviderTool {
        name: spec.name.to_owned(),
        description: spec.description.to_owned(),
        title: None,
        input_schema: action_input_schema(spec),
        output_schema: Some(action_output_schema(spec)),
        scope: spec.required_scope.map(ToOwned::to_owned),
        destructive: spec.destructive,
        requires_admin: spec.requires_admin,
        cost: Some(spec.cost.as_str().to_owned()),
        env: Vec::new(),
        limits: None,
        mcp: Some(McpOverlay {
            enabled: spec.transport.mcp(),
            title: None,
            annotations: json!({}),
        }),
        rest: static_rest_overlay(spec),
        cli: spec.cli.map(|cli| CliOverlay {
            enabled: spec.transport.cli(),
            command: Some(cli.command.to_owned()),
            aliases: Vec::new(),
            about: Some(cli.description.to_owned()),
            long_about: Some(cli.usage.to_owned()),
            hidden: false,
            flags: cli
                .flags
                .iter()
                .map(|flag| {
                    json!({
                        "name": flag.name,
                        "value_name": flag.value_name,
                        "required": flag.required,
                        "description": flag.description,
                    })
                })
                .collect(),
            default_output: None,
            interactive: false,
        }),
        palette: Some(PaletteOverlay {
            enabled: spec.transport != ActionTransport::McpOnly,
            category: Some("Example".to_owned()),
            icon: None,
            tone: Some("neutral".to_owned()),
            arg_mode: Some("schema".to_owned()),
            result_view: Some("json".to_owned()),
            aurora_blocks: Vec::new(),
        }),
        ui: None,
        examples: Vec::new(),
        meta: json!({
            "returns": spec.returns,
            "cli_usage": spec.cli.map(|cli| cli.usage),
            "scaffold_fallback": if spec.name == "scaffold_intent" {
                json!({
                    "recommended_skill": "scaffold-project",
                    "instructions": "Ask the user for the scaffold fields manually, then create the same JSON shape documented by the scaffold-project skill. Do not mutate files until the user approves the plan."
                })
            } else {
                Value::Null
            },
        }),
    }
}

fn static_rest_overlay(spec: &soma_domain::actions::ActionSpec) -> Option<RestOverlay> {
    match spec.rest_path {
        Some(path) => Some(RestOverlay {
            enabled: spec.transport.rest(),
            method: spec.rest_method.map(ToOwned::to_owned),
            path: Some(path.to_owned()),
            tags: vec!["soma".to_owned()],
            summary: Some(spec.description.to_owned()),
            description: Some(spec.description.to_owned()),
            deprecated: false,
            path_params: json!({}),
            query_params: json!({}),
            request_body_schema: None,
        }),
        None if !spec.transport.rest() => Some(RestOverlay {
            enabled: false,
            method: None,
            path: None,
            tags: vec!["soma".to_owned()],
            summary: Some(spec.description.to_owned()),
            description: Some(spec.description.to_owned()),
            deprecated: false,
            path_params: json!({}),
            query_params: json!({}),
            request_body_schema: None,
        }),
        None => None,
    }
}

fn action_output_schema(spec: &soma_domain::actions::ActionSpec) -> Value {
    match spec.name {
        "greet" => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["greeting", "target", "server"],
            "properties": {
                "greeting": { "type": "string" },
                "target": { "type": "string" },
                "server": { "type": "string" }
            }
        }),
        "echo" => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["echo"],
            "properties": {
                "echo": { "type": "string" }
            }
        }),
        "status" => json!({
            "type": "object",
            "additionalProperties": true,
            "required": ["status"],
            "properties": {
                "status": { "type": "string" },
                "note": { "type": "string" },
                "warnings": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            }
        }),
        "elicit_name" => json!({
            "type": "object",
            "additionalProperties": true,
            "properties": {
                "greeting": { "type": "string" },
                "name": { "type": "string" },
                "message": { "type": "string" },
                "note": { "type": "string" },
                "hint": { "type": "string" },
                "fallback_greeting": { "type": "string" }
            }
        }),
        "scaffold_intent" => json!({
            "type": "object",
            "additionalProperties": true,
            "properties": {
                "kind": { "type": "string" },
                "schema_version": { "type": "integer" },
                "status": { "type": "string" },
                "server_category": { "type": "string" },
                "required_surfaces": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "project": { "type": "object" },
                "upstream": { "type": "object" },
                "runtime": { "type": "object" },
                "mcp_primitives": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "handoff": { "type": "object" },
                "policy": { "type": "object" }
            }
        }),
        "help" => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["actions", "mcp_only_actions", "catalog", "preferred_rest_style", "usage", "examples"],
            "properties": {
                "actions": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "mcp_only_actions": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "catalog": {
                    "type": "array",
                    "items": { "type": "object" }
                },
                "preferred_rest_style": { "type": "string" },
                "usage": { "type": "string" },
                "examples": { "type": "object" }
            }
        }),
        _ => json!({
            "type": "object",
            "description": format!("Structured result for {}.", spec.returns),
            "additionalProperties": true
        }),
    }
}

fn action_input_schema(spec: &soma_domain::actions::ActionSpec) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for param in spec.params {
        let json_type = match param.ty {
            "integer" => "integer",
            "number" => "number",
            "boolean" => "boolean",
            "object" => "object",
            "array" => "array",
            _ => "string",
        };
        let mut schema = json!({
            "type": json_type,
            "description": param.description,
        });
        if param.required && json_type == "string" {
            schema["minLength"] = json!(1);
        }
        properties.insert(param.name.to_owned(), schema);
        if param.required {
            required.push(Value::String(param.name.to_owned()));
        }
    }
    let mut schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": properties,
    });
    if !required.is_empty() {
        schema["required"] = Value::Array(required);
    }
    schema
}

fn action_params(action: &str, params: &Value) -> Value {
    let mut params = params.clone();
    if let Value::Object(map) = &mut params {
        map.insert("action".to_owned(), Value::String(action.to_owned()));
    }
    params
}

fn surface_label(surface: crate::provider_registry::ProviderSurface) -> &'static str {
    match surface {
        crate::provider_registry::ProviderSurface::Mcp => "mcp",
        crate::provider_registry::ProviderSurface::Rest => "rest",
        crate::provider_registry::ProviderSurface::Cli => "cli",
        crate::provider_registry::ProviderSurface::Palette => "palette",
    }
}
