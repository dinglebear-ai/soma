//! CLI — thin shim that parses args, dispatches service actions, formats output.
//!
//! The CLI uses the same service layer as the MCP server. No business logic lives here.
//!
//! **Customize**: add subcommands to match your service's operations.
//!
//! # Usage
//!
//! ```text
//! soma greet --name Alice
//! soma echo --message "Hello!"
//! soma status
//! soma doctor [--json]
//! ```

use anyhow::{anyhow, Result};
use serde_json::Value;
use soma_application::{ExecuteActionRequest, ExecutionContext, SomaApplication};
use soma_cli_core::common_args::{
    parse_bool_flag as core_parse_bool_flag,
    parse_optional_value_flag as core_parse_optional_value_flag,
    parse_required_value_flag as core_parse_required_value_flag, reject_args as core_reject_args,
};
use soma_cli_core::confirmation::confirm_typed;
use soma_domain::actions::{ActionSpec, SomaAction};
use soma_domain::{Confirmation, RequestId, Surface};
use std::io::{BufRead, IsTerminal, Write};
use std::sync::{atomic::AtomicU64, atomic::Ordering, Arc};

// CUSTOMIZE: The doctor module is the §48 reference implementation.
//           Import it from here and wire into run() below.
pub mod doctor;
mod provider_command;
mod providers;
pub mod setup;
pub mod watch;

pub use provider_command::ProviderCommand;
use provider_command::{parse_providers_command, run_provider_management_command};
pub use setup::{apply_plugin_options, run_setup, SetupCommand};

pub const USAGE: &str = "Usage:
  soma mcp              Start MCP stdio transport
  soma serve            Start HTTP MCP + REST + Web server

  soma greet [--name NAME]       Greet NAME (or the world)
  soma echo --message MSG        Echo MSG back
  soma status                    Show server status
  soma help                      Show JSON action reference
  soma doctor [--json]           Run environment pre-flight checks
  soma watch [--url URL] [--interval N]  Poll /health and emit on state change
  soma setup check               Check plugin setup without mutating appdata
  soma setup repair              Create missing appdata/env setup files
  soma setup plugin-hook [--no-repair]  Plugin hook JSON contract
  soma providers validate        Validate provider manifests and compiled schemas
  soma providers inspect         Show provider manifests, surfaces, and capability posture
  soma providers test ACTION [--json JSON]  Dispatch one provider action through the registry
  soma providers list [--dir DIR] [--json]    List drop-in provider files (no execution)
  soma providers lint [--dir DIR] [--json]    Lint drop-in provider files (no execution)
  soma providers status [--dir DIR] [--json]  Summarize drop-in provider files (no execution)
  soma package generate [--write|--check]  Refresh generated provider docs, skills, and plugin metadata

  soma --help                    Show this help
  soma --version                 Show version

Environment:
  SOMA_API_URL          Deployed platform API or upstream service URL
  SOMA_API_KEY          Bearer token or upstream service API key
  SOMA_MCP_HOST         HTTP server bind host (default 127.0.0.1)
  SOMA_MCP_PORT         HTTP server bind port (default 40060)
  SOMA_MCP_NO_AUTH      Disable auth (loopback only)
  SOMA_MCP_TOKEN        Static bearer token
  RUST_LOG                 Log filter (e.g. info,rmcp=warn)";

pub fn usage() -> &'static str {
    USAGE
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Greet {
        name: Option<String>,
    },
    Echo {
        message: String,
    },
    Status,
    Help,
    /// Pre-flight environment validation (§48).
    ///
    /// CUSTOMIZE: Always keep this command. It is the operator's first stop
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
        /// Base URL of the MCP server (default: http://localhost:{SOMA_MCP_PORT}).
        url: Option<String>,
        /// Poll interval in seconds (default: 10).
        interval: u64,
    },
    Provider {
        command: String,
        json: Value,
    },
    Providers(ProviderCommand),
    PackageGenerate {
        write: bool,
    },
    Setup(SetupCommand),
}

pub struct CliInvocation {
    pub command: Command,
}

impl From<Command> for CliInvocation {
    fn from(command: Command) -> Self {
        Self { command }
    }
}

pub type CliError = anyhow::Error;

pub trait CliIo {
    fn stdout(&mut self, output: &str) -> Result<()>;
    fn stderr(&mut self, output: &str) -> Result<()>;
    fn confirm_destructive(&mut self, action: &str) -> Result<()>;
}

pub struct StandardCliIo;

impl CliIo for StandardCliIo {
    fn stdout(&mut self, output: &str) -> Result<()> {
        writeln!(std::io::stdout(), "{output}")?;
        Ok(())
    }

    fn stderr(&mut self, output: &str) -> Result<()> {
        writeln!(std::io::stderr(), "{output}")?;
        Ok(())
    }

    fn confirm_destructive(&mut self, action: &str) -> Result<()> {
        if !std::io::stdin().is_terminal() {
            return Err(anyhow!(
                "pass -y / --yes to confirm destructive action `{action}`"
            ));
        }
        confirm_destructive_action_from_io(
            action,
            &mut std::io::stdin().lock(),
            &mut std::io::stderr(),
        )
    }
}

/// Parse CLI arguments from `std::env::args()`.
///
/// Returns `None` if the first argument is not a known subcommand.
/// **Customize**: extend this to use clap or another arg parser for a real CLI.
/// This is intentionally minimal so Soma compiles without extra deps.
///
/// # CUSTOMIZE: Adding a new subcommand
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
            "greet" => {
                let name = parse_optional_value_flag(rest, "greet", "--name")?;
                Some(Command::Greet { name })
            }
            "echo" => {
                let message = parse_required_value_flag(rest, "echo", "--message")?
                    .filter(|m| !m.is_empty())
                    .ok_or_else(|| anyhow!("echo requires non-empty --message"))?;
                Some(Command::Echo { message })
            }
            "status" => {
                reject_args(rest, "status")?;
                Some(Command::Status)
            }
            "help" => {
                reject_args(rest, "help")?;
                Some(Command::Help)
            }
            // §48: doctor is always parsed here, dispatched via apps/soma::local::run
            // (called from lib.rs::run(), called from bin/soma.rs).
            // CUSTOMIZE: Keep this arm. It routes to doctor::run_doctor() which needs
            //           the full Config (not just SomaConfig), so apps/soma handles it.
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
            "package" => match rest {
                [action, flags @ ..] if action == "generate" => Some(Command::PackageGenerate {
                    write: parse_package_generate_flags(flags)?,
                }),
                _ => None,
            },
            "providers" => Some(parse_providers_command(rest)?),
            other => Some(parse_provider_command(other, rest)?),
        },
    };
    Ok(command)
}

/// Run a CLI command, print the result, and exit.
///
/// `Doctor`, `Watch`, `Setup`, and package generation are handled by the app
/// composition layer because they are process infrastructure rather than
/// product actions.
pub async fn run(
    application: Arc<SomaApplication>,
    invocation: CliInvocation,
    io: &mut dyn CliIo,
) -> Result<std::process::ExitCode, CliError> {
    let cmd = invocation.command;
    if let Command::Providers(command) = &cmd {
        if command.is_non_executing() {
            let Command::Providers(command) = cmd else {
                unreachable!()
            };
            providers::run_providers_command(command)?;
            return Ok(std::process::ExitCode::SUCCESS);
        }
    }

    let destructive_confirmed = confirm_command_if_destructive(&cmd, &application, io)?;

    if let Command::Providers(command) = &cmd {
        let result =
            run_provider_management_command(command, application.as_ref(), destructive_confirmed)
                .await?;
        io.stdout(&soma_cli_core::json::to_pretty_string(&result)?)?;
        return Ok(std::process::ExitCode::SUCCESS);
    }

    let request = match service_action_from_command(&cmd) {
        Some(action) => ExecuteActionRequest {
            action: action.name().to_owned(),
            params: cli_params(&action),
        },
        None if matches!(cmd, Command::Provider { .. }) => {
            let Command::Provider { json, .. } = &cmd else {
                unreachable!()
            };
            ExecuteActionRequest {
                action: provider_action_from_command(&cmd, &application)?,
                params: json.clone(),
            }
        }
        None => unreachable!("dispatched directly in apps/soma::local::run"),
    };
    let result = match application
        .execute_action(request, cli_execution_context(destructive_confirmed))
        .await
    {
        Ok(output) => output.output,
        Err(error) => {
            io.stderr(&soma_cli_core::json::to_pretty_string(&error)?)?;
            return Err(anyhow!(error.message));
        }
    };

    io.stdout(&soma_cli_core::json::to_pretty_string(&result)?)?;
    Ok(std::process::ExitCode::SUCCESS)
}

pub fn run_non_executing_provider_command(command: ProviderCommand) -> Result<()> {
    if !command.is_non_executing() {
        return Err(anyhow!(
            "provider command requires an initialized Soma application"
        ));
    }
    providers::run_providers_command(command)
}

pub(crate) fn cli_execution_context(destructive_confirmed: bool) -> ExecutionContext {
    static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);
    let sequence = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let request_id = RequestId::new(format!("cli-{}-{sequence}", std::process::id()))
        .expect("generated CLI request ids are valid");
    let mut context = ExecutionContext::loopback(Surface::Cli, request_id);
    context.destructive_confirmation = if destructive_confirmed {
        Confirmation::Confirmed
    } else {
        Confirmation::Missing
    };
    context
}

#[cfg(test)]
fn format_cli_tool_error(error: &soma_domain::errors::ToolError) -> String {
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

fn confirm_command_if_destructive(
    cmd: &Command,
    application: &SomaApplication,
    io: &mut dyn CliIo,
) -> Result<bool> {
    let Some(action) = command_action_name(cmd, application)? else {
        return Ok(false);
    };
    if !application.action_requires_confirmation(&action) {
        return Ok(false);
    }
    io.confirm_destructive(&action)?;
    Ok(true)
}

fn command_action_name(cmd: &Command, application: &SomaApplication) -> Result<Option<String>> {
    match cmd {
        Command::Provider { .. } => provider_action_from_command(cmd, application).map(Some),
        Command::Providers(ProviderCommand::Test { action, .. }) => Ok(Some(action.clone())),
        Command::Providers(_) => Ok(None),
        _ => Ok(service_action_from_command(cmd).map(|action| action.name().to_owned())),
    }
}

fn provider_action_from_command(cmd: &Command, application: &SomaApplication) -> Result<String> {
    let Command::Provider { command, .. } = cmd else {
        return Err(anyhow!("command is not a dynamic provider command"));
    };
    application
        .resolve_cli_action(command)
        .map_err(|_| anyhow!("unknown dynamic provider CLI command `{command}`"))
}

fn service_action_from_command(cmd: &Command) -> Option<SomaAction> {
    match cmd {
        Command::Greet { name } => Some(SomaAction::Greet { name: name.clone() }),
        Command::Echo { message } => Some(SomaAction::Echo {
            message: message.clone(),
        }),
        Command::Status => Some(SomaAction::Status),
        Command::Help => Some(SomaAction::Help),
        Command::Doctor { .. }
        | Command::Watch { .. }
        | Command::Provider { .. }
        | Command::Providers(_)
        | Command::PackageGenerate { .. }
        | Command::Setup(_) => None,
    }
}

fn cli_params(action: &SomaAction) -> serde_json::Value {
    match action {
        SomaAction::Greet { name } => match name {
            Some(name) => serde_json::json!({ "name": name }),
            None => serde_json::json!({}),
        },
        SomaAction::Echo { message } => serde_json::json!({ "message": message }),
        SomaAction::Status
        | SomaAction::Help
        | SomaAction::ElicitName
        | SomaAction::ScaffoldIntent => serde_json::json!({}),
    }
}

fn parse_provider_command(command: &str, args: &[String]) -> Result<Command> {
    if reserved_cli_command(command) {
        return Err(anyhow!("`{command}` is a reserved infrastructure command"));
    }
    match args {
        [flag, payload] if flag == "--json" => Ok(Command::Provider {
            command: command.to_owned(),
            json: serde_json::from_str(payload)
                .map_err(|error| anyhow!("{command} --json must be valid JSON: {error}"))?,
        }),
        [] => Ok(Command::Provider {
            command: command.to_owned(),
            json: serde_json::json!({}),
        }),
        _ => Ok(Command::Provider {
            command: command.to_owned(),
            json: parse_provider_flags(command, args)?,
        }),
    }
}

fn parse_package_generate_flags(args: &[String]) -> Result<bool> {
    match args {
        [] => Ok(false),
        [flag] if flag == "--check" => Ok(false),
        [flag] if flag == "--write" => Ok(true),
        [unexpected, ..] => Err(anyhow!(
            "package generate accepts only --write or --check, got `{unexpected}`"
        )),
    }
}

fn parse_provider_flags(command: &str, args: &[String]) -> Result<serde_json::Value> {
    let mut object = serde_json::Map::new();
    let mut chunks = args.chunks_exact(2);
    for pair in &mut chunks {
        let [flag, value] = pair else { unreachable!() };
        let key = flag
            .strip_prefix("--")
            .filter(|key| !key.is_empty())
            .ok_or_else(|| {
                anyhow!("{command} dynamic provider flags must use --name value pairs or --json")
            })?;
        object.insert(key.replace('-', "_"), scalar_json(value));
    }
    if !chunks.remainder().is_empty() {
        return Err(anyhow!(
            "{command} dynamic provider flags must use --name value pairs or --json"
        ));
    }
    Ok(serde_json::Value::Object(object))
}

fn scalar_json(value: &str) -> serde_json::Value {
    if value == "true" {
        serde_json::Value::Bool(true)
    } else if value == "false" {
        serde_json::Value::Bool(false)
    } else if let Ok(number) = value.parse::<i64>() {
        serde_json::Value::Number(number.into())
    } else if let Ok(number) = value.parse::<f64>() {
        serde_json::Number::from_f64(number)
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| serde_json::Value::String(value.to_owned()))
    } else {
        serde_json::Value::String(value.to_owned())
    }
}

// Must match soma_domain::provider_validation's RESERVED_CLI_COMMANDS
// exactly — that list is what soma providers validate/lint checks against,
// so a name reserved only here passes manifest validation but is
// unreachable once it hits this parser.
fn reserved_cli_command(command: &str) -> bool {
    matches!(
        command,
        "serve"
            | "mcp"
            | "doctor"
            | "watch"
            | "setup"
            | "package"
            | "tools"
            | "providers"
            | "openapi"
            | "help"
    )
}

pub fn run_package_generate(write: bool) -> Result<()> {
    let mode = if write { "--write" } else { "--check" };
    let mut command = std::process::Command::new("cargo");
    command.env_remove("CARGO_PROFILE_DEV_CODEGEN_BACKEND");
    let status = command
        .args(["xtask", "generate-provider-surfaces", mode])
        .status()
        .map_err(|error| {
            anyhow!("failed to run cargo xtask generate-provider-surfaces: {error}")
        })?;
    if !status.success() {
        return Err(anyhow!(
            "cargo xtask generate-provider-surfaces {mode} failed with {status}"
        ));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "ok": true,
            "changed": write,
            "command": "package generate",
            "mode": if write { "write" } else { "check" }
        }))?
    );
    Ok(())
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
    let prompt = format!("Action `{action}` is destructive. Type `{action}` to continue: ");
    if confirm_typed(writer, reader, &prompt, action)? {
        Ok(())
    } else {
        Err(anyhow!("aborted by user"))
    }
}

// ── arg parsing helpers ───────────────────────────────────────────────────────

fn reject_args(args: &[String], command: &str) -> Result<()> {
    Ok(core_reject_args(args, command)?)
}

fn parse_bool_flag(args: &[String], command: &str, flag: &str) -> Result<bool> {
    Ok(core_parse_bool_flag(args, command, flag)?)
}

fn parse_optional_value_flag(args: &[String], command: &str, flag: &str) -> Result<Option<String>> {
    Ok(core_parse_optional_value_flag(args, command, flag)?)
}

fn parse_required_value_flag(args: &[String], command: &str, flag: &str) -> Result<Option<String>> {
    Ok(core_parse_required_value_flag(args, command, flag)?)
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
