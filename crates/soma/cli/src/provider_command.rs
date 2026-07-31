//! `soma providers validate|inspect|test` — dispatches through the *live,
//! loaded application provider catalog; executes handlers.
//!
//! Distinct from the `providers` module (`soma providers list|lint|status`),
//! which is non-executing filesystem inspection that never touches the
//! registry.

use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use soma_application::{ExecuteActionRequest, SomaApplication};
use std::path::PathBuf;

use crate::{Command, cli_execution_context};

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
    Graduate {
        source: PathBuf,
        workspace: PathBuf,
        fixtures: Option<PathBuf>,
    },
    BuildComponent {
        workspace: PathBuf,
        component: Option<PathBuf>,
    },
    VerifyComponent {
        component: PathBuf,
    },
    Compare {
        component: PathBuf,
        fixtures: PathBuf,
    },
    Activate {
        workspace: PathBuf,
    },
    Rollback {
        workspace: PathBuf,
    },
}

impl ProviderCommand {
    /// The three non-executing variants never touch the live registry — they
    /// only parse manifests on disk, so `run()` short-circuits before any
    /// client/service/registry construction for these.
    pub fn is_non_executing(&self) -> bool {
        matches!(
            self,
            ProviderCommand::List { .. }
                | ProviderCommand::Lint { .. }
                | ProviderCommand::Status { .. }
                | ProviderCommand::Graduate { .. }
                | ProviderCommand::BuildComponent { .. }
                | ProviderCommand::VerifyComponent { .. }
                | ProviderCommand::Compare { .. }
                | ProviderCommand::Activate { .. }
                | ProviderCommand::Rollback { .. }
        )
    }
}

pub(crate) async fn run_provider_management_command(
    command: &ProviderCommand,
    application: &SomaApplication,
    destructive_confirmed: bool,
) -> Result<Value> {
    match command {
        ProviderCommand::Validate => Ok(application.provider_validation_summary()),
        ProviderCommand::Inspect => Ok(application.provider_inspection_report()),
        ProviderCommand::Test { action, json } => {
            let provider = application.provider_for_action(action);
            match application
                .execute_action(
                    ExecuteActionRequest {
                        action: action.clone(),
                        params: json.clone(),
                    },
                    cli_execution_context(destructive_confirmed),
                )
                .await
            {
                Ok(output) => Ok(json!({
                    "schema_version": 1,
                    "ok": true,
                    "action": action,
                    "provider": provider,
                    "result": output.output
                })),
                Err(error) => Err(anyhow!(error)),
            }
        }
        ProviderCommand::List { .. }
        | ProviderCommand::Lint { .. }
        | ProviderCommand::Status { .. }
        | ProviderCommand::Graduate { .. }
        | ProviderCommand::BuildComponent { .. }
        | ProviderCommand::VerifyComponent { .. }
        | ProviderCommand::Compare { .. }
        | ProviderCommand::Activate { .. }
        | ProviderCommand::Rollback { .. } => {
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
        [action, rest @ ..]
            if matches!(
                action.as_str(),
                "graduate"
                    | "build-component"
                    | "verify-component"
                    | "compare"
                    | "activate"
                    | "rollback"
            ) =>
        {
            Ok(Command::Providers(parse_graduation_command(action, rest)?))
        }
        [] => Err(anyhow!(
            "providers requires list, lint, status, validate, inspect, test, graduate, \
             build-component, verify-component, compare, activate, or rollback"
        )),
        [unexpected, ..] => Err(anyhow!("providers does not accept argument `{unexpected}`")),
    }
}

fn parse_graduation_command(command: &str, args: &[String]) -> Result<ProviderCommand> {
    let required = |flag: &str| -> Result<PathBuf> {
        args.windows(2)
            .find(|pair| pair[0] == flag)
            .map(|pair| PathBuf::from(&pair[1]))
            .ok_or_else(|| anyhow!("providers {command} {flag} requires a value"))
    };
    let expected = match command {
        "graduate" => &["--source", "--workspace", "--fixtures"][..],
        "build-component" => &["--workspace", "--component"][..],
        "verify-component" => &["--component"][..],
        "compare" => &["--component", "--fixtures"][..],
        "activate" | "rollback" => &["--workspace"][..],
        _ => unreachable!(),
    };
    let required_count = match command {
        "graduate" => 2,
        "build-component" => 1,
        _ => expected.len(),
    };
    if !args.len().is_multiple_of(2)
        || args.len() < required_count * 2
        || args.len() > expected.len() * 2
        || args
            .chunks_exact(2)
            .any(|pair| !expected.contains(&pair[0].as_str()) || pair[1].starts_with("--"))
        || args
            .chunks_exact(2)
            .map(|pair| pair[0].as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != args.len() / 2
    {
        return Err(anyhow!(
            "providers {command} expects {}",
            expected
                .iter()
                .map(|flag| format!("{flag} PATH"))
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    match command {
        "graduate" => Ok(ProviderCommand::Graduate {
            source: required("--source")?,
            workspace: required("--workspace")?,
            fixtures: args
                .windows(2)
                .find(|pair| pair[0] == "--fixtures")
                .map(|pair| PathBuf::from(&pair[1])),
        }),
        "build-component" => Ok(ProviderCommand::BuildComponent {
            workspace: required("--workspace")?,
            component: args
                .windows(2)
                .find(|pair| pair[0] == "--component")
                .map(|pair| PathBuf::from(&pair[1])),
        }),
        "verify-component" => Ok(ProviderCommand::VerifyComponent {
            component: required("--component")?,
        }),
        "compare" => Ok(ProviderCommand::Compare {
            component: required("--component")?,
            fixtures: required("--fixtures")?,
        }),
        "activate" => Ok(ProviderCommand::Activate {
            workspace: required("--workspace")?,
        }),
        "rollback" => Ok(ProviderCommand::Rollback {
            workspace: required("--workspace")?,
        }),
        _ => unreachable!(),
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
                ));
            }
        }
    }
    Ok((dir, json))
}

#[cfg(test)]
#[path = "provider_command_tests.rs"]
mod tests;
