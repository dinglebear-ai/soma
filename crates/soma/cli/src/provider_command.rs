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
        workspace: PathBuf,
        component: Option<PathBuf>,
        fixtures: PathBuf,
    },
    GraduationStatus {
        workspace: PathBuf,
    },
    Activate {
        workspace: PathBuf,
        confirmed: bool,
    },
    Rollback {
        workspace: PathBuf,
        confirmed: bool,
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
                    "result": output.output,
                    "request_id": output.request_id,
                    "progress": output.progress,
                })),
                Err(error) => Err(anyhow!(error)),
            }
        }
        ProviderCommand::Graduate {
            source,
            workspace,
            fixtures,
        } => {
            graduation_action(
                application,
                "graduate",
                workspace,
                Some(source),
                None,
                fixtures.as_ref(),
                destructive_confirmed,
            )
            .await
        }
        ProviderCommand::BuildComponent {
            workspace,
            component,
        } => {
            graduation_action(
                application,
                "build-component",
                workspace,
                None,
                component.as_ref(),
                None,
                destructive_confirmed,
            )
            .await
        }
        ProviderCommand::VerifyComponent { component } => {
            graduation_action(
                application,
                "verify-component",
                component
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new(".")),
                None,
                Some(component),
                None,
                destructive_confirmed,
            )
            .await
        }
        ProviderCommand::Compare {
            workspace,
            component,
            fixtures,
        } => {
            graduation_action(
                application,
                "compare",
                workspace,
                None,
                component.as_ref(),
                Some(fixtures),
                destructive_confirmed,
            )
            .await
        }
        ProviderCommand::GraduationStatus { workspace } => application
            .execute_action(
                ExecuteActionRequest {
                    action: "python_graduation_status".to_owned(),
                    params: json!({"workspace": workspace}),
                },
                cli_execution_context(destructive_confirmed),
            )
            .await
            .map(|response| response.into_surface_value())
            .map_err(anyhow::Error::from),
        ProviderCommand::Activate { workspace, .. } => {
            graduation_action(
                application,
                "activate",
                workspace,
                None,
                None,
                None,
                destructive_confirmed,
            )
            .await
        }
        ProviderCommand::Rollback { workspace, .. } => {
            graduation_action(
                application,
                "rollback",
                workspace,
                None,
                None,
                None,
                destructive_confirmed,
            )
            .await
        }
        ProviderCommand::List { .. }
        | ProviderCommand::Lint { .. }
        | ProviderCommand::Status { .. } => {
            unreachable!("non-executing provider commands are handled before registry construction")
        }
    }
}

async fn graduation_action(
    application: &SomaApplication,
    operation: &str,
    workspace: &std::path::Path,
    source: Option<&std::path::PathBuf>,
    component: Option<&std::path::PathBuf>,
    fixtures: Option<&std::path::PathBuf>,
    confirmed: bool,
) -> Result<Value> {
    let mut params = json!({
        "operation": operation,
        "workspace": workspace,
        "confirm": confirmed,
    });
    let object = params.as_object_mut().expect("object");
    if let Some(source) = source {
        object.insert("source".to_owned(), json!(source));
    }
    if let Some(component) = component {
        object.insert("component".to_owned(), json!(component));
    }
    if let Some(fixtures) = fixtures {
        object.insert("fixtures".to_owned(), json!(fixtures));
    }
    application
        .execute_action(
            ExecuteActionRequest {
                action: "python_graduation_apply".to_owned(),
                params,
            },
            cli_execution_context(confirmed),
        )
        .await
        .map(|response| response.into_surface_value())
        .map_err(anyhow::Error::from)
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
                    | "graduation-status"
                    | "activate"
                    | "rollback"
            ) =>
        {
            Ok(Command::Providers(parse_graduation_command(action, rest)?))
        }
        [] => Err(anyhow!(
            "providers requires list, lint, status, validate, inspect, test, graduate, \
             build-component, verify-component, compare, graduation-status, activate, or rollback"
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
        "compare" => &["--workspace", "--component", "--fixtures"][..],
        "graduation-status" => &["--workspace"][..],
        "activate" | "rollback" => &["--workspace", "--confirm"][..],
        _ => unreachable!(),
    };
    let required_count = match command {
        "graduate" | "compare" => 2,
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
            workspace: required("--workspace")?,
            component: args
                .windows(2)
                .find(|pair| pair[0] == "--component")
                .map(|pair| PathBuf::from(&pair[1])),
            fixtures: required("--fixtures")?,
        }),
        "graduation-status" => Ok(ProviderCommand::GraduationStatus {
            workspace: required("--workspace")?,
        }),
        "activate" => Ok(ProviderCommand::Activate {
            workspace: required("--workspace")?,
            confirmed: args
                .windows(2)
                .find(|pair| pair[0] == "--confirm")
                .is_some_and(|pair| pair[1] == "true"),
        }),
        "rollback" => Ok(ProviderCommand::Rollback {
            workspace: required("--workspace")?,
            confirmed: args
                .windows(2)
                .find(|pair| pair[0] == "--confirm")
                .is_some_and(|pair| pair[1] == "true"),
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
