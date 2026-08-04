use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::Value;
use synapse_application::LegacyTool;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

use crate::{ExecuteOptions, StandaloneError, StandaloneRuntime, SynapseConfig};

#[derive(Debug, Parser)]
#[command(
    name = "synapse",
    version,
    about = "Canonical infrastructure operations runtime"
)]
struct Cli {
    #[arg(long, env = "SYNAPSE_CONFIG", global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true)]
    compact: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Serve {
        #[arg(long)]
        bind: Option<String>,
    },
    Mcp,
    Operations,
    Plan(OperationArgs),
    Run(OperationArgs),
    Legacy(LegacyArgs),
}

#[derive(Debug, Args)]
struct OperationArgs {
    operation: String,
    #[arg(long, default_value = "{}", conflicts_with = "params_file")]
    params: String,
    #[arg(long)]
    params_file: Option<PathBuf>,
    #[arg(long)]
    yes: bool,
    #[arg(long)]
    idempotency_key: Option<String>,
    #[arg(long)]
    actor: Option<String>,
}

#[derive(Debug, Args)]
struct LegacyArgs {
    #[arg(value_enum)]
    tool: LegacyToolArg,
    #[arg(long, default_value = "{}", conflicts_with = "input_file")]
    input: String,
    #[arg(long)]
    input_file: Option<PathBuf>,
    #[arg(long)]
    yes: bool,
    #[arg(long)]
    idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum LegacyToolArg {
    Flux,
    Scout,
}

impl From<LegacyToolArg> for LegacyTool {
    fn from(value: LegacyToolArg) -> Self {
        match value {
            LegacyToolArg::Flux => Self::Flux,
            LegacyToolArg::Scout => Self::Scout,
        }
    }
}

pub async fn run<I, T>(args: I) -> anyhow::Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    init_tracing();
    let cli = Cli::try_parse_from(args)?;
    let mut config = SynapseConfig::load(cli.config.as_deref())?;
    if let Command::Serve { bind: Some(bind) } = &cli.command {
        config.server.bind = bind.clone();
    }
    let runtime = Arc::new(StandaloneRuntime::from_config(config)?);
    let outcome = match cli.command {
        Command::Serve { .. } => crate::http::serve(Arc::clone(&runtime)).await,
        Command::Mcp => crate::mcp::serve_stdio(Arc::clone(&runtime)).await,
        Command::Operations => {
            print_value(&runtime.operation_catalog_json(), cli.compact)?;
            Ok(())
        }
        Command::Plan(args) => run_plan(&runtime, args, cli.compact).await,
        Command::Run(args) => run_operation(&runtime, args, cli.compact).await,
        Command::Legacy(args) => run_legacy(&runtime, args, cli.compact).await,
    };
    runtime.shutdown().await;
    outcome
}

async fn run_plan(
    runtime: &StandaloneRuntime,
    args: OperationArgs,
    compact: bool,
) -> anyhow::Result<()> {
    let parameters = input_value(&args.params, args.params_file.as_deref())?;
    let options = options(&args);
    let plan = runtime.plan(&args.operation, &parameters, &options).await?;
    print_value(&serde_json::to_value(plan)?, compact)
}

async fn run_operation(
    runtime: &StandaloneRuntime,
    args: OperationArgs,
    compact: bool,
) -> anyhow::Result<()> {
    let parameters = input_value(&args.params, args.params_file.as_deref())?;
    let options = options(&args);
    match runtime
        .execute(
            &args.operation,
            &parameters,
            &options,
            &CancellationToken::new(),
        )
        .await
    {
        Ok(value) => print_value(&value, compact),
        Err(error @ StandaloneError::ConfirmationRequired(_)) => {
            let plan = error.plan().expect("confirmation error carries plan");
            print_value(
                &serde_json::json!({
                    "error": "confirmation_required",
                    "message": "review the plan and rerun with --yes",
                    "plan": plan,
                }),
                compact,
            )?;
            anyhow::bail!("mutation confirmation required")
        }
        Err(error) => Err(error.into()),
    }
}

async fn run_legacy(
    runtime: &StandaloneRuntime,
    args: LegacyArgs,
    compact: bool,
) -> anyhow::Result<()> {
    let input = input_value(&args.input, args.input_file.as_deref())?;
    let options = ExecuteOptions {
        confirmed: args.yes,
        idempotency_key: args.idempotency_key,
        actor: Some("legacy-cli".into()),
    };
    let value = runtime
        .execute_legacy(
            args.tool.into(),
            &input,
            &options,
            &CancellationToken::new(),
        )
        .await?;
    print_value(&value, compact)
}

fn options(args: &OperationArgs) -> ExecuteOptions {
    ExecuteOptions {
        confirmed: args.yes,
        idempotency_key: args.idempotency_key.clone(),
        actor: args.actor.clone().or_else(|| Some("cli".into())),
    }
}

fn input_value(inline: &str, file: Option<&Path>) -> anyhow::Result<Value> {
    let text = if let Some(path) = file {
        std::fs::read_to_string(path)
            .map_err(|error| anyhow::anyhow!("cannot read {}: {error}", path.display()))?
    } else {
        inline.to_owned()
    };
    let value: Value = serde_json::from_str(&text)?;
    if !value.is_object() {
        anyhow::bail!("operation input must be a JSON object");
    }
    Ok(value)
}

fn print_value(value: &Value, compact: bool) -> anyhow::Result<()> {
    if compact {
        println!("{}", serde_json::to_string(value)?);
    } else {
        println!("{}", serde_json::to_string_pretty(value)?);
    }
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
