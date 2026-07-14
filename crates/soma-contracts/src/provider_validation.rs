use std::collections::BTreeSet;

use jsonschema::Validator;
use serde_json::Value;

use crate::providers::{HostCapabilities, ProviderKind, ProviderManifest};

const SCHEMA: &str = include_str!("../../../docs/contracts/provider-manifest.schema.json");
// Must match crates/soma-cli/src/lib.rs's `reserved_cli_command()` exactly —
// that's the actual gate a provider's `tools[].cli.command` has to clear to
// be dispatchable at all. A name missing here passes manifest/lint
// validation but is unreachable through the CLI once it hits that parser.
const RESERVED_CLI_COMMANDS: &[&str] = &[
    "serve",
    "mcp",
    "doctor",
    "watch",
    "setup",
    "package",
    "tools",
    "providers",
    "openapi",
    "help",
];

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

        if let Some(rest) = &tool.rest {
            if rest.enabled {
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
        }

        if let Some(cli) = &tool.cli {
            if cli.enabled {
                let command = cli.command.as_deref().unwrap_or(tool.name.as_str());
                validate_cli_command(manifest.provider.kind, &tool.name, command)?;
                if !cli_commands.insert(command.to_owned()) {
                    return Err(ProviderValidationError::new(
                        "duplicate_cli_command",
                        format!("duplicate CLI command `{command}`"),
                    ));
                }
                for alias in &cli.aliases {
                    validate_cli_command(manifest.provider.kind, &tool.name, alias)?;
                    if !cli_commands.insert(alias.clone()) {
                        return Err(ProviderValidationError::new(
                            "duplicate_cli_command",
                            format!("duplicate CLI alias `{alias}`"),
                        ));
                    }
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

fn validate_cli_command(
    provider_kind: ProviderKind,
    tool_name: &str,
    command: &str,
) -> Result<(), ProviderValidationError> {
    if provider_kind == ProviderKind::StaticRust && tool_name == "help" && command == "help" {
        return Ok(());
    }
    if RESERVED_CLI_COMMANDS.contains(&command) {
        return Err(ProviderValidationError::new(
            "reserved_cli_command",
            format!("provider command `{command}` is reserved"),
        ));
    }
    Ok(())
}

fn validate_env_name(name: &str) -> Result<(), ProviderValidationError> {
    if name.starts_with("SOMA_") || name.starts_with("LAB_") {
        return Err(ProviderValidationError::new(
            "invalid_env_declaration",
            format!("env declaration `{name}` must be logical and unprefixed"),
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

#[cfg(test)]
#[path = "provider_validation_tests.rs"]
mod tests;
