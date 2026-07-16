use std::collections::BTreeSet;

use jsonschema::Validator;
use serde_json::Value;

use crate::{HostCapabilities, ProviderManifest};

const SCHEMA: &str = include_str!("../provider-manifest.schema.json");

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error("{code}: {message}")]
pub struct ProviderValidationError {
    code: &'static str,
    message: String,
}

impl ProviderValidationError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

pub fn validate_provider_manifest_value(
    value: &Value,
) -> Result<ProviderManifest, ProviderValidationError> {
    validate_manifest_schema(value)?;
    let manifest: ProviderManifest = serde_json::from_value(value.clone()).map_err(|error| {
        ProviderValidationError::new("manifest_deserialize_failed", error.to_string())
    })?;
    validate_provider_manifest(&manifest)?;
    Ok(manifest)
}

pub fn validate_manifest_schema(value: &Value) -> Result<(), ProviderValidationError> {
    let schema: Value = serde_json::from_str(SCHEMA)
        .map_err(|error| ProviderValidationError::new("schema_parse_failed", error.to_string()))?;
    let compiled: Validator = jsonschema::options().build(&schema).map_err(|error| {
        ProviderValidationError::new("schema_compile_failed", error.to_string())
    })?;
    let details = compiled
        .iter_errors(value)
        .map(|error| format!("{}: {}", error.instance_path(), error))
        .collect::<Vec<_>>();
    if !details.is_empty() {
        return Err(ProviderValidationError::new(
            "json_schema_failed",
            details.join("; "),
        ));
    }
    Ok(())
}

pub fn validate_provider_manifest(
    manifest: &ProviderManifest,
) -> Result<(), ProviderValidationError> {
    let value = serde_json::to_value(manifest).map_err(|error| {
        ProviderValidationError::new("manifest_serialize_failed", error.to_string())
    })?;
    let mut compatibility_value = value;
    normalize_typed_legacy_nulls(&mut compatibility_value);
    validate_manifest_schema(&compatibility_value)?;

    let mut tool_names = BTreeSet::new();
    let mut rest_routes = BTreeSet::new();
    let mut cli_commands = BTreeSet::new();
    let mut primitive_names = BTreeSet::new();

    for tool in &manifest.tools {
        reject_instruction_injection("tool.description", &tool.description)?;
        if !tool_names.insert(tool.name.as_str()) {
            return Err(ProviderValidationError::new(
                "duplicate_tool_name",
                format!("duplicate tool name `{}`", tool.name),
            ));
        }
        if !matches!(
            tool.input_schema.get("type").and_then(Value::as_str),
            Some("object")
        ) {
            return Err(ProviderValidationError::new(
                "missing_input_schema",
                format!("tool `{}` must declare an object input_schema", tool.name),
            ));
        }

        if let Some(rest) = &tool.rest
            && rest.enabled
        {
            let method = rest.method.as_deref().unwrap_or("POST");
            let path = rest
                .path
                .clone()
                .unwrap_or_else(|| format!("/v1/{}", tool.name));
            if !rest_routes.insert((method.to_owned(), path.clone())) {
                return Err(ProviderValidationError::new(
                    "duplicate_rest_route",
                    format!("duplicate REST route {method} {path}"),
                ));
            }
        }

        if let Some(cli) = &tool.cli
            && cli.enabled
        {
            let command = cli.command.as_deref().unwrap_or(tool.name.as_str());
            validate_cli_command(command)?;
            if !cli_commands.insert(command.to_owned()) {
                return Err(ProviderValidationError::new(
                    "duplicate_cli_command",
                    format!("duplicate CLI command `{command}`"),
                ));
            }
            for alias in &cli.aliases {
                validate_cli_command(alias)?;
                if !cli_commands.insert(alias.clone()) {
                    return Err(ProviderValidationError::new(
                        "duplicate_cli_command",
                        format!("duplicate CLI alias `{alias}`"),
                    ));
                }
            }
        }

        for env in &tool.env {
            validate_env_name(&env.name)?;
        }
    }

    for env in &manifest.env {
        validate_env_name(&env.name)?;
    }
    validate_capabilities(&manifest.capabilities)?;

    for prompt in &manifest.prompts {
        reject_instruction_injection("prompt.description", &prompt.description)?;
        insert_primitive(&mut primitive_names, "prompt", &prompt.name)?;
    }
    for resource in &manifest.resources {
        reject_instruction_injection("resource.description", &resource.description)?;
        insert_primitive(&mut primitive_names, "resource", &resource.name)?;
    }
    for task in &manifest.tasks {
        reject_instruction_injection("task.description", &task.description)?;
        insert_primitive(&mut primitive_names, "task", &task.name)?;
        if !matches!(
            task.input_schema.get("type").and_then(Value::as_str),
            Some("object")
        ) {
            return Err(ProviderValidationError::new(
                "missing_input_schema",
                format!("task `{}` must declare an object input_schema", task.name),
            ));
        }
    }
    for elicitation in &manifest.elicitation {
        reject_instruction_injection("elicitation.description", &elicitation.description)?;
        insert_primitive(&mut primitive_names, "elicitation", &elicitation.name)?;
    }

    if let Some(docs) = &manifest.docs {
        if let Some(when_to_use) = &docs.when_to_use {
            reject_instruction_injection("docs.when_to_use", when_to_use)?;
        }
        for entry in &docs.troubleshooting {
            reject_instruction_injection("docs.troubleshooting", entry)?;
        }
    }

    Ok(())
}

fn normalize_typed_legacy_nulls(value: &mut Value) {
    let Some(root) = value.as_object_mut() else {
        return;
    };
    remove_null_fields(root, &["docs", "plugin", "ui", "meta"]);
    normalize_object_field(root, "provider", |provider| {
        remove_null_fields(
            provider,
            &[
                "title",
                "description",
                "homepage",
                "source",
                "version",
                "enabled",
            ],
        );
    });
    normalize_array_field(root, "tools", normalize_tool);
    normalize_array_field(root, "prompts", normalize_prompt);
    normalize_array_field(root, "resources", normalize_resource);
    normalize_array_field(root, "tasks", normalize_task);
    normalize_array_field(root, "elicitation", normalize_elicitation);
    normalize_array_field(root, "env", normalize_env_requirement);
    normalize_object_field(root, "capabilities", normalize_capabilities);
    normalize_object_field(root, "docs", |docs| {
        remove_null_fields(docs, &["when_to_use"]);
        normalize_array_field(docs, "examples", normalize_example);
    });
    normalize_object_field(root, "plugin", |plugin| {
        remove_null_fields(plugin, &["mcp_registration"]);
    });
    normalize_object_field(root, "ui", normalize_ui);
}

fn normalize_tool(tool: &mut serde_json::Map<String, Value>) {
    remove_null_fields(
        tool,
        &[
            "title",
            "output_schema",
            "scope",
            "cost",
            "limits",
            "mcp",
            "rest",
            "cli",
            "palette",
            "ui",
            "meta",
        ],
    );
    normalize_array_field(tool, "env", normalize_env_requirement);
    normalize_object_field(tool, "limits", normalize_limits);
    normalize_object_field(tool, "mcp", normalize_mcp);
    normalize_object_field(tool, "rest", normalize_rest);
    normalize_object_field(tool, "cli", normalize_cli);
    normalize_object_field(tool, "palette", normalize_palette);
    normalize_object_field(tool, "ui", normalize_ui);
    normalize_array_field(tool, "examples", normalize_example);
}

fn normalize_prompt(prompt: &mut serde_json::Map<String, Value>) {
    remove_null_fields(prompt, &["template", "arguments_schema", "scope", "mcp"]);
    normalize_object_field(prompt, "mcp", normalize_mcp);
    normalize_array_field(prompt, "examples", normalize_example);
}

fn normalize_resource(resource: &mut serde_json::Map<String, Value>) {
    remove_null_fields(resource, &["mime_type", "scope", "mcp", "annotations"]);
    normalize_object_field(resource, "mcp", normalize_mcp);
}

fn normalize_task(task: &mut serde_json::Map<String, Value>) {
    remove_null_fields(task, &["output_schema", "scope", "mcp", "limits"]);
    normalize_object_field(task, "mcp", normalize_mcp);
    normalize_object_field(task, "limits", normalize_limits);
}

fn normalize_elicitation(elicitation: &mut serde_json::Map<String, Value>) {
    remove_null_fields(elicitation, &["scope", "mcp"]);
    normalize_object_field(elicitation, "mcp", normalize_mcp);
}

fn normalize_env_requirement(env: &mut serde_json::Map<String, Value>) {
    remove_null_fields(env, &["description", "default"]);
}

fn normalize_capabilities(capabilities: &mut serde_json::Map<String, Value>) {
    remove_null_fields(
        capabilities,
        &[
            "filesystem",
            "network",
            "env",
            "terminal",
            "browser",
            "github",
        ],
    );
    normalize_object_field(capabilities, "terminal", |terminal| {
        remove_null_fields(terminal, &["working_dir"]);
    });
}

fn normalize_mcp(mcp: &mut serde_json::Map<String, Value>) {
    remove_null_fields(mcp, &["title", "annotations"]);
}

fn normalize_rest(rest: &mut serde_json::Map<String, Value>) {
    remove_null_fields(
        rest,
        &[
            "method",
            "path",
            "summary",
            "description",
            "path_params",
            "query_params",
            "request_body_schema",
        ],
    );
}

fn normalize_cli(cli: &mut serde_json::Map<String, Value>) {
    remove_null_fields(cli, &["command", "about", "long_about", "default_output"]);
}

fn normalize_palette(palette: &mut serde_json::Map<String, Value>) {
    remove_null_fields(
        palette,
        &["category", "icon", "tone", "arg_mode", "result_view"],
    );
}

fn normalize_ui(ui: &mut serde_json::Map<String, Value>) {
    remove_null_fields(ui, &["meta"]);
}

fn normalize_limits(limits: &mut serde_json::Map<String, Value>) {
    remove_null_fields(
        limits,
        &["timeout_ms", "max_response_bytes", "max_input_bytes"],
    );
}

fn normalize_example(example: &mut serde_json::Map<String, Value>) {
    remove_null_fields(
        example,
        &[
            "title",
            "description",
            "input",
            "output",
            "cli",
            "rest",
            "mcp",
        ],
    );
}

fn remove_null_fields(object: &mut serde_json::Map<String, Value>, fields: &[&str]) {
    for field in fields {
        if object.get(*field).is_some_and(Value::is_null) {
            object.remove(*field);
        }
    }
}

fn normalize_object_field(
    object: &mut serde_json::Map<String, Value>,
    field: &str,
    normalize: impl FnOnce(&mut serde_json::Map<String, Value>),
) {
    if let Some(Value::Object(value)) = object.get_mut(field) {
        normalize(value);
    }
}

fn normalize_array_field(
    object: &mut serde_json::Map<String, Value>,
    field: &str,
    normalize: fn(&mut serde_json::Map<String, Value>),
) {
    if let Some(Value::Array(values)) = object.get_mut(field) {
        for value in values {
            if let Value::Object(value) = value {
                normalize(value);
            }
        }
    }
}

fn validate_cli_command(command: &str) -> Result<(), ProviderValidationError> {
    if command.trim().is_empty() {
        return Err(ProviderValidationError::new(
            "invalid_cli_command",
            "provider command must not be empty",
        ));
    }
    Ok(())
}

fn validate_env_name(name: &str) -> Result<(), ProviderValidationError> {
    if name.trim().is_empty() {
        return Err(ProviderValidationError::new(
            "invalid_env_declaration",
            "env declaration name must not be empty",
        ));
    }
    Ok(())
}

fn validate_capabilities(capabilities: &HostCapabilities) -> Result<(), ProviderValidationError> {
    if capabilities
        .filesystem
        .as_ref()
        .map(|cap| cap.enabled && cap.read_roots.is_empty() && cap.write_roots.is_empty())
        .unwrap_or(false)
    {
        return Err(ProviderValidationError::new(
            "empty_capability_scope",
            "enabled filesystem capability must declare at least one read or write root",
        ));
    }
    if capabilities
        .network
        .as_ref()
        .map(|cap| cap.enabled && cap.allowed_hosts.is_empty())
        .unwrap_or(false)
    {
        return Err(ProviderValidationError::new(
            "empty_capability_scope",
            "enabled network capability must declare at least one allowed host",
        ));
    }
    if capabilities
        .env
        .as_ref()
        .map(|cap| cap.enabled && cap.allowed.is_empty())
        .unwrap_or(false)
    {
        return Err(ProviderValidationError::new(
            "empty_capability_scope",
            "enabled env capability must declare at least one allowed variable",
        ));
    }
    if capabilities
        .terminal
        .as_ref()
        .map(|cap| cap.enabled && cap.working_dir.is_none() && cap.allowlist.is_empty())
        .unwrap_or(false)
    {
        return Err(ProviderValidationError::new(
            "empty_capability_scope",
            "enabled terminal capability must declare a working directory or allowlist",
        ));
    }
    if capabilities
        .browser
        .as_ref()
        .map(|cap| cap.enabled && cap.allowed_origins.is_empty())
        .unwrap_or(false)
    {
        return Err(ProviderValidationError::new(
            "empty_capability_scope",
            "enabled browser capability must declare at least one allowed origin",
        ));
    }
    if capabilities
        .github
        .as_ref()
        .map(|cap| cap.enabled && cap.allowed_repos.is_empty())
        .unwrap_or(false)
    {
        return Err(ProviderValidationError::new(
            "empty_capability_scope",
            "enabled github capability must declare at least one allowed repo",
        ));
    }
    Ok(())
}

fn insert_primitive<'a>(
    names: &mut BTreeSet<&'a str>,
    kind: &str,
    name: &'a str,
) -> Result<(), ProviderValidationError> {
    if !names.insert(name) {
        return Err(ProviderValidationError::new(
            "duplicate_mcp_primitive",
            format!("duplicate MCP primitive `{name}` in {kind} catalog"),
        ));
    }
    Ok(())
}

fn reject_instruction_injection(field: &str, value: &str) -> Result<(), ProviderValidationError> {
    let lower = value.to_ascii_lowercase();
    let blocked = [
        "ignore previous instructions",
        "ignore all previous",
        "system prompt",
        "developer message",
        "you are now",
    ];
    if blocked.iter().any(|needle| lower.contains(needle)) {
        return Err(ProviderValidationError::new(
            "generated_doc_prompt_injection",
            format!("{field} contains instruction-like text that generated docs must not elevate"),
        ));
    }
    Ok(())
}
