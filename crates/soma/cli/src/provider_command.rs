//! `soma providers validate|inspect|test` — dispatches through the *live,
//! loaded* `ProviderRegistry`; executes handlers.
//!
//! Distinct from the `providers` module (`soma providers list|lint|status`),
//! which is non-executing filesystem inspection that never touches the
//! registry.

use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use soma_service::{
    ProviderAuthMode, ProviderCall, ProviderPrincipal, ProviderRegistry, ProviderRequestLimits,
    ProviderSurface,
};
use std::path::PathBuf;

use crate::Command;

#[derive(Debug, PartialEq, Eq)]
pub enum ProviderCommand {
    Validate,
    Inspect,
    Test {
        action: String,
        json: Value,
    },
    /// Non-executing: lists drop-in provider files without loading the registry.
    List {
        dir: Option<PathBuf>,
        json: bool,
    },
    /// Non-executing: lints drop-in provider files without loading the registry.
    Lint {
        dir: Option<PathBuf>,
        json: bool,
    },
    /// Non-executing: summarizes drop-in provider files without loading the registry.
    Status {
        dir: Option<PathBuf>,
        json: bool,
    },
}

impl ProviderCommand {
    /// The three non-executing variants never touch the live registry — they
    /// only parse manifests on disk, so `run()` short-circuits before any
    /// client/service/registry construction for these.
    pub(crate) fn is_non_executing(&self) -> bool {
        matches!(
            self,
            ProviderCommand::List { .. }
                | ProviderCommand::Lint { .. }
                | ProviderCommand::Status { .. }
        )
    }
}

pub(crate) async fn run_provider_management_command(
    command: &ProviderCommand,
    registry: &ProviderRegistry,
    destructive_confirmed: bool,
) -> Result<Value> {
    match command {
        ProviderCommand::Validate => Ok(registry.snapshot().validation_summary()),
        ProviderCommand::Inspect => Ok(registry.snapshot().inspection_report()),
        ProviderCommand::Test { action, json } => {
            let provider = registry
                .snapshot()
                .provider_for_action(action)
                .map(str::to_owned);
            match registry
                .dispatch(ProviderCall {
                    provider: String::new(),
                    action: action.clone(),
                    params: json.clone(),
                    principal: ProviderPrincipal::loopback_dev(),
                    auth_mode: ProviderAuthMode::LoopbackDev,
                    surface: ProviderSurface::Cli,
                    destructive_confirmed,
                    limits: ProviderRequestLimits::default(),
                    snapshot_id: String::new(),
                })
                .await
            {
                Ok(output) => Ok(json!({
                    "schema_version": 1,
                    "ok": true,
                    "action": action,
                    "provider": provider,
                    "result": output.value
                })),
                Err(error) => {
                    eprintln!("{}", serde_json::to_string_pretty(&error)?);
                    Err(anyhow!(error.message))
                }
            }
        }
        ProviderCommand::List { .. }
        | ProviderCommand::Lint { .. }
        | ProviderCommand::Status { .. } => {
            unreachable!("non-executing provider commands are handled before registry construction")
        }
    }
}

pub(crate) fn parse_providers_command(args: &[String]) -> Result<Command> {
    match args {
        [action] if action == "validate" => Ok(Command::Providers(ProviderCommand::Validate)),
        [action] if action == "inspect" => Ok(Command::Providers(ProviderCommand::Inspect)),
        [action, provider_action] if action == "test" => {
            Ok(Command::Providers(ProviderCommand::Test {
                action: provider_action.clone(),
                json: json!({}),
            }))
        }
        [action, provider_action, flag, payload] if action == "test" && flag == "--json" => {
            Ok(Command::Providers(ProviderCommand::Test {
                action: provider_action.clone(),
                json: serde_json::from_str(payload).map_err(|error| {
                    anyhow!("providers test {provider_action} --json must be valid JSON: {error}")
                })?,
            }))
        }
        [action, rest @ ..] if action == "list" || action == "lint" || action == "status" => {
            let (dir, json) = parse_providers_dir_flags(action, rest)?;
            Ok(Command::Providers(match action.as_str() {
                "list" => ProviderCommand::List { dir, json },
                "lint" => ProviderCommand::Lint { dir, json },
                _ => ProviderCommand::Status { dir, json },
            }))
        }
        [] => Err(anyhow!(
            "providers requires list, lint, status, validate, inspect, or test ACTION"
        )),
        [unexpected, ..] => Err(anyhow!("providers does not accept argument `{unexpected}`")),
    }
}

fn parse_providers_dir_flags(command: &str, args: &[String]) -> Result<(Option<PathBuf>, bool)> {
    let mut dir = None;
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--dir" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| anyhow!("providers {command} --dir requires a value"))?;
                if value.starts_with("--") {
                    return Err(anyhow!("providers {command} --dir requires a value"));
                }
                dir = Some(PathBuf::from(value));
                index += 2;
            }
            "--json" => {
                json = true;
                index += 1;
            }
            unknown => {
                return Err(anyhow!(
                    "providers {command} does not accept argument `{unknown}`"
                ))
            }
        }
    }
    Ok((dir, json))
}

#[cfg(test)]
#[path = "provider_command_tests.rs"]
mod tests;
