//! Binary runtime helpers shared by the local and server entrypoints.
//!
//! Binaries decide which modes they expose. The functions here keep the actual
//! CLI, stdio MCP, and HTTP server wiring in one place.

use anyhow::Result;
#[cfg(feature = "mcp-http")]
use std::sync::Arc;

#[cfg(feature = "mcp-stdio")]
use rmcp::{transport::stdio, ServiceExt};
#[cfg(feature = "mcp-http")]
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

use rtemplate_contracts::config::Config;
#[cfg(any(feature = "mcp-stdio", feature = "mcp-http"))]
use rtemplate_service::{ExampleClient, ExampleService};

#[cfg(feature = "cli")]
use rtemplate_cli as cli;
#[cfg(feature = "mcp-stdio")]
use rtemplate_mcp as mcp;
#[cfg(feature = "mcp-http")]
use rtemplate_runtime::server::{resolve_auth_policy_kind, AuthPolicyKind};
#[cfg(all(feature = "mcp", not(feature = "mcp-stdio")))]
use rtemplate_runtime::server::{AppState, AuthPolicy};
#[cfg(feature = "mcp-stdio")]
use rtemplate_runtime::server::{AppState, AuthPolicy};

pub fn init_logging(stdio_mode: bool, serve_mode: bool) {
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(default_log_level(stdio_mode, serve_mode))),
        )
        .with_writer(std::io::stderr)
        .with_target(true)
        .init();
}

fn default_log_level(stdio_mode: bool, serve_mode: bool) -> &'static str {
    if stdio_mode || !serve_mode {
        "warn"
    } else {
        "info"
    }
}

/// Start the MCP HTTP server (Streamable HTTP transport).
#[cfg(feature = "mcp-http")]
pub async fn serve_http_mcp() -> Result<()> {
    let config = Config::load()?;
    let state = build_state(config).await?;

    info!(
        bind = %state.config.bind_addr(),
        server_name = %state.config.server_name,
        auth = ?state.auth_policy,
        "rtemplate-mcp starting"
    );

    let bind = state.config.bind_addr();
    let app = crate::routes::router(state).layer(tower_http::trace::TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    info!(bind = %bind, "MCP HTTP server listening");

    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Start the MCP stdio transport (for local/subprocess MCP clients).
///
/// Stdio is always LoopbackDev: it is a local trusted pipe between parent and
/// child process. HTTP auth middleware does not apply.
#[cfg(feature = "mcp-stdio")]
pub async fn serve_stdio_mcp() -> Result<()> {
    let config = Config::load()?;
    let service = ExampleService::new(ExampleClient::new(&config.example)?);
    let state = AppState {
        config: config.mcp,
        auth_policy: AuthPolicy::LoopbackDev,
        service,
        response_pages: Default::default(),
    };
    let svc = mcp::rmcp_server(state).serve(stdio()).await?;
    svc.waiting().await?;
    Ok(())
}

/// Dispatch CLI subcommands.
#[cfg(feature = "cli")]
pub async fn run_cli() -> Result<()> {
    let parsed = cli::parse_args()?;
    // Translate CLAUDE_PLUGIN_OPTION_* into RTEMPLATE_* env vars BEFORE Config::load()
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
        Some(cmd) => cli::run(cmd, &config.example).await,
        None => {
            eprintln!("Unknown command. Run `example --help` for usage.");
            std::process::exit(1);
        }
    }
}

#[cfg(feature = "mcp-http")]
async fn build_state(config: Config) -> Result<AppState> {
    let auth_policy = build_auth_policy(&config).await?;
    let service = ExampleService::new(ExampleClient::new(&config.example)?);
    Ok(AppState {
        config: config.mcp,
        auth_policy,
        service,
        response_pages: Default::default(),
    })
}

#[cfg(feature = "mcp-http")]
async fn build_auth_policy(config: &Config) -> Result<AuthPolicy> {
    match resolve_auth_policy_kind(config, config.mcp.trusted_gateway)? {
        AuthPolicyKind::LoopbackDev => Ok(AuthPolicy::LoopbackDev),
        AuthPolicyKind::TrustedGatewayUnscoped => Ok(AuthPolicy::TrustedGatewayUnscoped),
        AuthPolicyKind::MountedBearer => Ok(AuthPolicy::Mounted { auth_state: None }),
        AuthPolicyKind::MountedOAuth => {
            let auth_cfg = rtemplate_auth::config::AuthConfigBuilder::new()
                .env_prefix("RTEMPLATE_MCP")
                .session_cookie_name("example_mcp_session")
                .scopes_supported(vec![
                    rtemplate_contracts::actions::READ_SCOPE.into(),
                    rtemplate_contracts::actions::WRITE_SCOPE.into(),
                ])
                .default_scope("example:read")
                .resource_path("/mcp")
                .enable_dynamic_registration(true)
                .build_from_sources(std::env::vars())
                .map_err(|e| anyhow::anyhow!("OAuth config error: {e}"))?;
            let auth_state = rtemplate_auth::state::AuthState::new(auth_cfg)
                .await
                .map_err(|e| anyhow::anyhow!("OAuth state init error: {e}"))?;
            Ok(AuthPolicy::Mounted {
                auth_state: Some(Arc::new(auth_state)),
            })
        }
    }
}

#[cfg(feature = "mcp-http")]
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %e, "CTRL+C handler failed");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => {
                tracing::error!(error = %e, "SIGTERM handler failed");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
    tracing::info!("Shutdown signal received");
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
