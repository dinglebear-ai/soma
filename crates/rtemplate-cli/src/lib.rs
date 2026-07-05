//! CLI — thin shim that parses args, dispatches service actions, formats output.
//!
//! The CLI uses the same service layer as the MCP server. No business logic lives here.
//!
//! **Template**: add subcommands to match your service's operations.
//!
//! # Usage
//!
//! ```text
//! example greet --name Alice
//! example echo --message "Hello!"
//! example status
//! example doctor [--json]
//! ```

use anyhow::{anyhow, Result};
use rtemplate_contracts::{actions::ActionSpec, config::ExampleConfig};
use rtemplate_service::{classify_service_error, dispatch_action, ExampleClient, ExampleService};
use serde_json::{Map, Value};
use std::io::{BufRead, IsTerminal, Write};

// TEMPLATE: The doctor module is the §48 reference implementation.
//           Import it from here and wire into run() below.
pub mod doctor;
pub mod setup;
pub mod watch;

pub use setup::{apply_plugin_options, run_setup, SetupCommand};

pub const USAGE: &str = "Usage:
  example mcp              Start MCP stdio transport
  example-server [serve]   Start HTTP MCP + REST + Web server

  example greet [--name NAME]       Greet NAME (or the world)
  example echo --message MSG        Echo MSG back
  example status                    Show server status
  example help                      Show JSON action reference
  example doctor [--json]           Run environment pre-flight checks
  example watch [--url URL] [--interval N]  Poll /health and emit on state change
  example setup check               Check plugin setup without mutating appdata
  example setup repair              Create missing appdata/env setup files
  example setup plugin-hook [--no-repair]  Plugin hook JSON contract

  example --help                    Show this help
  example --version                 Show version

Environment:
  RTEMPLATE_API_URL          Deployed platform API or upstream service URL
  RTEMPLATE_API_KEY          Bearer token or upstream service API key
  RTEMPLATE_MCP_HOST         HTTP server bind host (default 127.0.0.1)
  RTEMPLATE_MCP_PORT         HTTP server bind port (default 40060)
  RTEMPLATE_MCP_NO_AUTH      Disable auth (loopback only)
  RTEMPLATE_MCP_TOKEN        Static bearer token
  RUST_LOG                 Log filter (e.g. info,rmcp=warn)";

pub fn usage() -> &'static str {
    USAGE
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Action {
        name: String,
        params: Value,
        yes: bool,
    },
    /// Pre-flight environment validation (§48).
    ///
    /// TEMPLATE: Always keep this command. It is the operator's first stop
    /// when setting up or debugging the service.
    Doctor {
        /// Output JSON instead of human-readable text.
        json: bool,
    },
    /// Poll the MCP server health endpoint and emit a line on every state change.
    ///
    /// Designed to be run as a plugin monitor — stdout is the event stream,
    /// stderr is debug output. Exits only on CTRL+C.
    Watch {
        /// Base URL of the MCP server (default: http://localhost:{RTEMPLATE_MCP_PORT}).
        url: Option<String>,
        /// Poll interval in seconds (default: 10).
        interval: u64,
    },
    Setup(SetupCommand),
}

/// Parse CLI arguments from `std::env::args()`.
///
/// Returns `None` if the first argument is not a known subcommand.
/// **Template**: extend this to use clap or another arg parser for a real CLI.
/// This is intentionally minimal so the template compiles without extra deps.
///
/// # TEMPLATE: Adding a new subcommand
///
/// 1. Add a variant to `Command` above.
/// 2. Add a match arm here to construct it from args.
/// 3. Add a dispatch arm in `run()` below.
/// 4. Update `USAGE` above.
pub fn parse_args() -> Result<Option<Command>> {
    parse_args_from(std::env::args().skip(1))
}

pub fn parse_args_from<I, S>(args: I) -> Result<Option<Command>>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    let command = match args.as_slice() {
        [] => None,
        [subcommand, rest @ ..] => match subcommand.as_str() {
            // §48: doctor is always parsed here, dispatched via run_cli in main.rs.
            // TEMPLATE: Keep this arm. It routes to doctor::run_doctor() which needs
            //           the full Config (not just ExampleConfig), so main.rs handles it.
            "doctor" => {
                let json = parse_bool_flag(rest, "doctor", "--json")?;
                Some(Command::Doctor { json })
            }
            "watch" => {
                let (url, interval_arg) = parse_watch_flags(rest)?;
                let interval = match interval_arg {
                    Some(v) => v.parse().map_err(|_| {
                        anyhow!("watch --interval must be a positive integer number of seconds")
                    })?,
                    None => 10,
                };
                if interval == 0 {
                    return Err(anyhow!(
                        "watch --interval must be a positive integer number of seconds"
                    ));
                }
                Some(Command::Watch { url, interval })
            }
            "setup" => match rest {
                [action, flags @ ..] if action == "check" => {
                    reject_args(flags, "setup check")?;
                    Some(Command::Setup(SetupCommand::Check))
                }
                [action, flags @ ..] if action == "repair" => {
                    reject_args(flags, "setup repair")?;
                    Some(Command::Setup(SetupCommand::Repair))
                }
                [action, flags @ ..] if action == "install" => {
                    reject_args(flags, "setup install")?;
                    Some(Command::Setup(SetupCommand::Install))
                }
                [action, flags @ ..] if action == "plugin-hook" => {
                    let no_repair = parse_bool_flag(flags, "setup plugin-hook", "--no-repair")?;
                    Some(Command::Setup(SetupCommand::PluginHook { no_repair }))
                }
                _ => None,
            },
            _ => parse_dynamic_action_command(subcommand, rest)?,
        },
    };
    Ok(command)
}

/// Run a CLI command, print the result, and exit.
///
/// # TEMPLATE
/// - `Doctor` is handled specially in `main.rs::run_cli` (needs full `Config`).
/// - All other commands get only `ExampleConfig`; keep it that way.
/// - Add `--json` support to each new command by forwarding a `json` flag.
pub async fn run(cmd: Command, cfg: &ExampleConfig) -> Result<()> {
    let client = ExampleClient::new(cfg)?;
    let service = ExampleService::new(client);
    confirm_command_if_destructive(&cmd)?;

    let result = match &cmd {
        Command::Action { name, params, .. } => {
            match dispatch_action(&service, name, params, "cli").await {
                Ok(value) => value,
                Err(error) => {
                    let tool_error = classify_service_error(&error);
                    eprintln!("{}", format_cli_tool_error(&tool_error));
                    return Err(anyhow!(tool_error.message));
                }
            }
        }
        // Doctor, Watch, and Setup are never dispatched via this function — main.rs
        // handles them directly because they need config.mcp fields.
        Command::Doctor { .. } | Command::Watch { .. } | Command::Setup(_) => {
            unreachable!("dispatched directly in main.rs::run_cli")
        }
    };

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn format_cli_tool_error(error: &rtemplate_contracts::errors::ToolError) -> String {
    let mut lines = vec![
        format!("error: {}", error.message),
        format!("code: {}", error.code),
        format!("kind: {}", error.kind.as_str()),
        format!("retryable: {}", error.retryable),
        format!("remediation: {}", error.remediation),
    ];
    if let Some(field) = &error.field {
        lines.push(format!("field: {field}"));
    }
    if let Some(bad_value) = &error.bad_value {
        lines.push(format!("bad_value: {bad_value}"));
    }
    lines.join("\n")
}

fn confirm_command_if_destructive(cmd: &Command) -> Result<()> {
    let Command::Action { name, yes, .. } = cmd else {
        return Ok(());
    };
    confirm_destructive_action_allowed(
        rtemplate_service::action_specs(),
        name,
        *yes,
        std::io::stdin().is_terminal(),
    )
}

pub fn confirm_destructive_action_allowed(
    actions: &[ActionSpec],
    action: &str,
    yes: bool,
    stdin_is_terminal: bool,
) -> Result<()> {
    if yes
        || !actions
            .iter()
            .any(|spec| spec.name == action && spec.destructive)
    {
        return Ok(());
    }
    if !stdin_is_terminal {
        return Err(anyhow!(
            "pass -y / --yes to confirm destructive action `{action}`"
        ));
    }
    confirm_destructive_action_from_io(action, &mut std::io::stdin().lock(), &mut std::io::stderr())
}

fn confirm_destructive_action_from_io<R, W>(
    action: &str,
    reader: &mut R,
    writer: &mut W,
) -> Result<()>
where
    R: BufRead,
    W: Write,
{
    write!(
        writer,
        "Action `{action}` is destructive. Type `{action}` to continue: "
    )?;
    writer.flush()?;

    let mut input = String::new();
    reader.read_line(&mut input)?;
    if input.trim() == action {
        Ok(())
    } else {
        Err(anyhow!("aborted by user"))
    }
}

// ── arg parsing helpers ───────────────────────────────────────────────────────

fn reject_args(args: &[String], command: &str) -> Result<()> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(anyhow!("{command} does not accept argument `{}`", args[0]))
    }
}

fn parse_bool_flag(args: &[String], command: &str, flag: &str) -> Result<bool> {
    let mut found = false;
    for arg in args {
        if arg == flag {
            if found {
                return Err(anyhow!("{command} received duplicate {flag}"));
            }
            found = true;
        } else {
            return Err(anyhow!("{command} does not accept argument `{arg}`"));
        }
    }
    Ok(found)
}

fn parse_dynamic_action_command(action: &str, rest: &[String]) -> Result<Option<Command>> {
    let Some(spec) = rtemplate_service::action_registry().cli_command(action) else {
        return Ok(None);
    };
    let Some(cli) = spec.cli else {
        return Ok(None);
    };
    let mut params = Map::new();
    let mut yes = false;
    let mut index = 0;
    while index < rest.len() {
        let flag = rest[index].as_str();
        if flag == "--yes" || flag == "-y" {
            if yes {
                return Err(anyhow!("duplicate flag {flag} for action {action}"));
            }
            yes = true;
            index += 1;
            continue;
        }
        let Some(flag_spec) = cli.flags.iter().find(|candidate| candidate.name == flag) else {
            return Err(anyhow!("unknown flag {flag} for action {action}"));
        };
        let key = flag.trim_start_matches("--");
        if params.contains_key(key) {
            return Err(anyhow!("duplicate flag {flag} for action {action}"));
        }
        let Some(value) = rest.get(index + 1) else {
            return Err(anyhow!(
                "{action} {flag} requires {}",
                flag_spec.value_name.unwrap_or("VALUE")
            ));
        };
        if value.starts_with('-') {
            return Err(anyhow!("{action} {flag} value looks like a flag: {value}"));
        }
        params.insert(key.to_owned(), Value::String(value.clone()));
        index += 2;
    }
    for flag in cli.flags.iter().filter(|flag| flag.required) {
        let key = flag.name.trim_start_matches("--");
        if !params.contains_key(key) {
            return Err(anyhow!("missing required flag {}", flag.name));
        }
    }
    let params = Value::Object(params);
    rtemplate_service::validate_params(spec, &params)?;
    Ok(Some(Command::Action {
        name: spec.name.to_owned(),
        params,
        yes,
    }))
}

fn parse_watch_flags(args: &[String]) -> Result<(Option<String>, Option<String>)> {
    let mut url = None;
    let mut interval = None;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let target = match flag {
            "--url" => &mut url,
            "--interval" => &mut interval,
            _ => return Err(anyhow!("watch does not accept argument `{flag}`")),
        };
        if target.is_some() {
            return Err(anyhow!("watch received duplicate {flag}"));
        }
        let Some(value) = args.get(index + 1) else {
            return Err(anyhow!("watch requires a value after {flag}"));
        };
        if value.starts_with("--") {
            return Err(anyhow!("watch requires a value after {flag}"));
        }
        *target = Some(value.clone());
        index += 2;
    }
    Ok((url, interval))
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
