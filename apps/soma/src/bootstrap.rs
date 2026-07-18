//! Builds the concrete Soma dependency graph.
//!
//! This module is the only place `apps/soma` constructs engines: it loads
//! `SomaConfig`, builds the transport client and provider registry, wires
//! gateway/Code Mode adapters into `ApplicationPorts`, and constructs
//! `SomaApplication` and `SomaRuntime`. `local.rs`, `http.rs`, and `stdio.rs`
//! call into these constructors — they never build engines themselves (plan
//! section 3.1).

use std::sync::Arc;

use anyhow::Result;
#[cfg(any(feature = "cli", feature = "mcp-stdio", feature = "mcp-http"))]
use soma_client::SomaClient;
#[cfg(any(feature = "cli", feature = "mcp-stdio", feature = "mcp-http"))]
use soma_config::Config;
#[cfg(any(feature = "cli", feature = "mcp-stdio", feature = "mcp-http"))]
use soma_service::SomaService;

use soma_application::{ApplicationPorts, SomaApplication};
#[cfg(any(feature = "mcp-stdio", feature = "mcp-http"))]
use soma_runtime::server::gateway_product_state_from_env;
#[cfg(feature = "mcp")]
use soma_runtime::server::AppState;
#[cfg(any(feature = "mcp-stdio", feature = "mcp-http", feature = "oauth"))]
use soma_runtime::server::AuthPolicy;
#[cfg(feature = "mcp-http")]
use soma_runtime::server::{resolve_auth_policy_kind, AuthPolicyKind};
#[cfg(any(
    feature = "mcp-stdio",
    feature = "mcp-http",
    all(
        any(test, feature = "test-support"),
        any(feature = "cli", feature = "mcp", feature = "api")
    )
))]
use soma_runtime::server::{GatewayProductState, SomaRuntime};
#[cfg(any(
    feature = "mcp-stdio",
    feature = "mcp-http",
    all(
        any(test, feature = "test-support"),
        any(feature = "cli", feature = "mcp", feature = "api")
    )
))]
use soma_service::ProviderRegistry;
#[cfg(all(feature = "cli", feature = "mcp-stdio"))]
use tracing_subscriber::{fmt, EnvFilter};

/// Initialize tracing at `level` unless `RUST_LOG` overrides it.
///
/// Only called from `run()` (gated `cli` + `mcp-stdio`) before it dispatches
/// to a mode — never from `http::serve()` directly, since `tracing_subscriber`
/// panics if a global default is installed twice. A downstream fork that
/// embeds `soma::server::serve_http_mcp()` under `mcp-http` alone (bypassing
/// `run()`) is responsible for initializing its own tracing subscriber, same
/// as pre-PR18's `serve_http_mcp()` never called this either — it was always
/// `bin/soma.rs`'s `main()` that did.
///
/// Stdio mode always runs at `warn` (see `crate::invocation::DispatchMode`)
/// so JSON-RPC framing on stdout is never corrupted by log lines; the HTTP
/// server defaults to `info`.
#[cfg(all(feature = "cli", feature = "mcp-stdio"))]
pub(crate) fn init_logging(level: &str) {
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level)),
        )
        .with_writer(std::io::stderr)
        .with_target(true)
        .init();
}

/// Build `SomaApplication`'s ports from `soma-integrations` adapters and wrap
/// it with `SomaRuntime`. The only constructor for `SomaRuntime` — every mode
/// that needs a runtime goes through it.
#[cfg(any(
    feature = "mcp-stdio",
    feature = "mcp-http",
    all(
        any(test, feature = "test-support"),
        any(feature = "cli", feature = "mcp", feature = "api")
    )
))]
pub(crate) fn runtime_for_components(
    service: SomaService,
    provider_registry: ProviderRegistry,
    gateway: GatewayProductState,
) -> Arc<SomaRuntime> {
    let ports = ApplicationPorts::unavailable()
        .with_gateway(Arc::new(soma_integrations::GatewayApplicationPort::new(
            gateway.clone(),
        )))
        .with_codemode(Arc::new(
            soma_integrations::CodeModeApplicationPort::default(),
        ));
    let application = Arc::new(SomaApplication::new(
        Arc::new(service),
        Arc::new(provider_registry),
        ports,
    ));
    Arc::new(SomaRuntime::new(application, gateway))
}

#[cfg(feature = "mcp")]
pub(crate) fn authorization_mode(state: &AppState) -> soma_domain::AuthorizationMode {
    match &state.auth_policy {
        soma_runtime::server::AuthPolicy::LoopbackDev => {
            soma_domain::AuthorizationMode::LoopbackDev
        }
        soma_runtime::server::AuthPolicy::TrustedGatewayUnscoped => {
            soma_domain::AuthorizationMode::TrustedGateway
        }
        soma_runtime::server::AuthPolicy::Mounted { .. } => soma_domain::AuthorizationMode::Mounted,
    }
}

#[cfg(feature = "mcp")]
pub(crate) fn mcp_state_for_state(state: &AppState) -> soma_mcp::McpState {
    soma_mcp::McpState::new(
        state.application_handle(),
        state.config.clone(),
        authorization_mode(state),
        state.response_pages.clone(),
    )
}

/// Build the `Arc<SomaApplication>` a one-shot CLI command runs against.
#[cfg(feature = "cli")]
pub(crate) async fn cli_application(config: &Config) -> Result<Arc<SomaApplication>> {
    cli_application_with_provider_dir(config, None).await
}

#[cfg(feature = "cli")]
pub(crate) async fn cli_application_with_provider_dir(
    config: &Config,
    provider_dir: Option<&std::path::Path>,
) -> Result<Arc<SomaApplication>> {
    let service = SomaService::new(SomaClient::new(&config.soma)?);
    let registry = if config.soma.is_remote_adapter() {
        soma_service::remote_provider_registry(service.clone()).await?
    } else {
        let registry = match provider_dir {
            Some(provider_dir) => {
                soma_service::dynamic_provider_registry_from_dir(service.clone(), provider_dir)?
            }
            None => soma_service::dynamic_provider_registry(service.clone())?,
        };
        registry
            .refresh_file_providers()
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        registry
    };
    Ok(Arc::new(SomaApplication::new(
        Arc::new(service),
        Arc::new(registry),
        ApplicationPorts::unavailable(),
    )))
}

/// Build the stdio MCP `AppState`. Stdio is always `AuthPolicy::LoopbackDev`:
/// it is a local trusted pipe between parent and child process, so HTTP auth
/// middleware does not apply.
#[cfg(feature = "mcp-stdio")]
pub(crate) async fn stdio_state() -> Result<AppState> {
    let config = Config::load()?;
    let service = SomaService::new(SomaClient::new(&config.soma)?);
    let remote_adapter = config.soma.is_remote_adapter();
    let provider_registry = if remote_adapter {
        soma_service::remote_provider_registry(service.clone()).await?
    } else {
        soma_service::dynamic_provider_registry(service.clone())?
    };
    let gateway = gateway_product_state_from_env()?;
    #[cfg(feature = "oauth")]
    configure_gateway_upstream_oauth_from_env(&gateway).await?;
    let runtime = runtime_for_components(service, provider_registry, gateway);
    Ok(AppState::new(
        config.mcp,
        AuthPolicy::LoopbackDev,
        runtime,
        Default::default(),
    ))
}

/// Build the HTTP MCP/REST `AppState`, resolving the auth policy from config.
#[cfg(feature = "mcp-http")]
pub(crate) async fn http_state() -> Result<AppState> {
    let config = Config::load()?;
    let auth_policy = http_auth_policy(&config).await?;
    let service = SomaService::new(SomaClient::new(&config.soma)?);
    let provider_registry = soma_service::dynamic_provider_registry(service.clone())?;
    let gateway = gateway_product_state_from_env()?;
    #[cfg(feature = "oauth")]
    configure_gateway_upstream_oauth_for_policy(&gateway, &auth_policy).await?;
    let runtime = runtime_for_components(service, provider_registry, gateway);
    Ok(AppState::new(
        config.mcp,
        auth_policy,
        runtime,
        Default::default(),
    ))
}

#[cfg(feature = "oauth")]
async fn configure_gateway_upstream_oauth_for_policy(
    gateway: &soma_runtime::server::GatewayProductState,
    auth_policy: &AuthPolicy,
) -> Result<()> {
    if !gateway_has_oauth_upstreams(gateway) {
        return Ok(());
    }
    if let AuthPolicy::Mounted {
        auth_state: Some(auth_state),
    } = auth_policy
    {
        return configure_gateway_upstream_oauth(gateway, auth_state.config.as_ref()).await;
    }
    let auth_config = soma_integrations::auth::soma_auth_config_builder()
        .build_from_sources(std::env::vars())
        .map_err(|error| anyhow::anyhow!("Gateway upstream OAuth config error: {error}"))?;
    configure_gateway_upstream_oauth(gateway, &auth_config).await
}

#[cfg(all(feature = "oauth", feature = "mcp-stdio"))]
async fn configure_gateway_upstream_oauth_from_env(
    gateway: &soma_runtime::server::GatewayProductState,
) -> Result<()> {
    if !gateway_has_oauth_upstreams(gateway) {
        return Ok(());
    }
    let auth_config = soma_integrations::auth::soma_auth_config_builder()
        .build_from_sources(std::env::vars())
        .map_err(|error| anyhow::anyhow!("Gateway upstream OAuth config error: {error}"))?;
    configure_gateway_upstream_oauth(gateway, &auth_config).await
}

#[cfg(feature = "oauth")]
async fn configure_gateway_upstream_oauth(
    gateway: &soma_runtime::server::GatewayProductState,
    auth_config: &soma_auth::config::AuthConfig,
) -> Result<()> {
    let key = std::env::var("SOMA_MCP_OAUTH_ENCRYPTION_KEY").ok();
    let upstreams = gateway
        .config_view()
        .upstream
        .iter()
        .filter_map(|upstream| gateway.upstream_config(&upstream.name))
        .collect::<Vec<_>>();
    if let Some(runtime) =
        soma_integrations::gateway_auth::build_runtime(&upstreams, auth_config, key.as_deref())
            .await?
    {
        gateway.install_upstream_oauth_runtime(runtime);
    }
    Ok(())
}

#[cfg(feature = "oauth")]
fn gateway_has_oauth_upstreams(gateway: &soma_runtime::server::GatewayProductState) -> bool {
    gateway
        .config_view()
        .upstream
        .iter()
        .any(|upstream| upstream.oauth_enabled)
}

#[cfg(feature = "mcp-http")]
async fn http_auth_policy(config: &Config) -> Result<AuthPolicy> {
    match resolve_auth_policy_kind(config, config.mcp.trusted_gateway)? {
        AuthPolicyKind::LoopbackDev => Ok(AuthPolicy::LoopbackDev),
        AuthPolicyKind::TrustedGatewayUnscoped => Ok(AuthPolicy::TrustedGatewayUnscoped),
        AuthPolicyKind::MountedBearer => Ok(mounted_bearer_policy()),
        AuthPolicyKind::MountedOAuth => {
            let auth_cfg = soma_integrations::auth::soma_auth_config_builder()
                .build_from_sources(std::env::vars())
                .map_err(|e| anyhow::anyhow!("OAuth config error: {e}"))?;
            let auth_state = soma_auth::state::AuthState::new(auth_cfg)
                .await
                .map_err(|e| anyhow::anyhow!("OAuth state init error: {e}"))?;
            Ok(AuthPolicy::Mounted {
                auth_state: Some(Arc::new(auth_state)),
            })
        }
    }
}

#[cfg(all(feature = "mcp-http", feature = "auth"))]
fn mounted_bearer_policy() -> AuthPolicy {
    AuthPolicy::Mounted { auth_state: None }
}

#[cfg(all(feature = "mcp-http", not(feature = "auth")))]
fn mounted_bearer_policy() -> AuthPolicy {
    AuthPolicy::Mounted {}
}

#[cfg(test)]
#[path = "bootstrap_tests.rs"]
mod tests;
