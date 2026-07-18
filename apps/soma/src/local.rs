//! Runs one-shot CLI commands.
//!
//! `run()` parses `argv` through `soma-cli`, loads `SomaConfig`, and
//! dispatches each command — building the `Arc<SomaApplication>` a command
//! needs through `bootstrap::cli_application` first. The CLI shim
//! (`soma-cli`) implements each command; this module only wires the composed
//! application in (plan section 3.1).

use anyhow::Result;
use soma_cli as cli;
use soma_config::Config;

/// Dispatch a CLI subcommand.
pub(crate) async fn run() -> Result<()> {
    let parsed = cli::parse_args()?;
    // Translate CLAUDE_PLUGIN_OPTION_* into SOMA_* env vars BEFORE Config::load()
    // so the plugin hook can call the binary directly (no plugin-setup.sh wrapper).
    if matches!(
        parsed,
        Some(cli::Command::Setup(cli::SetupCommand::PluginHook { .. }))
    ) {
        cli::apply_plugin_options();
    }
    let config = Config::load()?;
    match parsed {
        Some(cli::Command::Doctor { json }) => cli::doctor::run_doctor(&config, json).await,
        Some(cli::Command::Watch { url, interval }) => {
            let base = url.unwrap_or_else(|| format!("http://localhost:{}", config.mcp.port));
            cli::watch::run_watch(&base, interval).await
        }
        Some(cli::Command::Setup(command)) => cli::run_setup(&config, command).await,
        Some(cli::Command::PackageGenerate { write }) => cli::run_package_generate(write),
        Some(cli::Command::Providers(command)) if command.is_non_executing() => {
            cli::run_non_executing_provider_command(command)
        }
        Some(cmd) => {
            let application = crate::bootstrap::cli_application(&config).await?;
            let mut io = cli::StandardCliIo;
            cli::run(application, cmd.into(), &mut io).await?;
            Ok(())
        }
        None => {
            eprintln!("Unknown command. Run `soma --help` for usage.");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
