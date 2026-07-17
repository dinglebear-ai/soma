//! Soma policy layered over provider-core's transport-neutral validation.
//!
//! `soma-provider-core` (shared layer) validates manifests generically and
//! must stay product-agnostic. The extra policy here — reserved Soma CLI
//! verbs, `SOMA_`/`LAB_`-prefixed env rejection — is Soma-specific, so it
//! lives in `soma-domain` instead of being folded into the shared crate.

use serde_json::Value;

use soma_provider_core::{ProviderKind, ProviderManifest};

pub use soma_provider_core::ProviderValidationError;

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
    soma_provider_core::validate_manifest_schema(value)
}

pub fn validate_provider_manifest(
    manifest: &ProviderManifest,
) -> Result<(), ProviderValidationError> {
    soma_provider_core::validate_provider_manifest(manifest)?;
    validate_soma_cli_policy(manifest)?;
    for env in manifest
        .env
        .iter()
        .chain(manifest.tools.iter().flat_map(|tool| tool.env.iter()))
    {
        if env.name.starts_with("SOMA_") || env.name.starts_with("LAB_") {
            return Err(ProviderValidationError::new(
                "invalid_env_declaration",
                format!(
                    "env declaration `{}` must be logical and unprefixed",
                    env.name
                ),
            ));
        }
    }
    Ok(())
}

fn validate_soma_cli_policy(manifest: &ProviderManifest) -> Result<(), ProviderValidationError> {
    for tool in &manifest.tools {
        let Some(cli) = &tool.cli else {
            continue;
        };
        if !cli.enabled {
            continue;
        }
        let command = cli.command.as_deref().unwrap_or(&tool.name);
        for candidate in std::iter::once(command).chain(cli.aliases.iter().map(String::as_str)) {
            if manifest.provider.kind == ProviderKind::StaticRust
                && tool.name == "help"
                && candidate == "help"
            {
                continue;
            }
            if RESERVED_CLI_COMMANDS.contains(&candidate) {
                return Err(ProviderValidationError::new(
                    "reserved_cli_command",
                    format!("provider command `{candidate}` is reserved"),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "provider_validation_tests.rs"]
mod tests;
