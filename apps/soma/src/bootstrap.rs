//! Builds the concrete Soma dependency graph.
//!
//! This module is the only place `apps/soma` constructs engines: it loads
//! `SomaConfig`, builds the transport client and provider registry, wires
//! gateway/Code Mode adapters into `ApplicationPorts`, and constructs
//! `SomaApplication` and `SomaRuntime`. `local.rs`, `http.rs`, and `stdio.rs`
//! call into these constructors — they never build engines themselves (plan
//! section 3.1).

use std::sync::Arc;

use anyhow::{Context, Result, bail};
#[cfg(any(feature = "cli", feature = "mcp-stdio", feature = "mcp-http"))]
use soma_application::SomaService;
#[cfg(any(feature = "cli", feature = "mcp-stdio", feature = "mcp-http"))]
use soma_client::SomaClient;
#[cfg(any(feature = "cli", feature = "mcp-stdio", feature = "mcp-http"))]
use soma_config::Config;

#[cfg(any(
    feature = "mcp-stdio",
    feature = "mcp-http",
    all(
        any(test, feature = "test-support"),
        any(feature = "cli", feature = "mcp", feature = "api")
    )
))]
use soma_application::ProviderRegistry;
use soma_application::{ApplicationPorts, PythonEnvironmentPort, SomaApplication};
#[cfg(feature = "mcp")]
use soma_runtime::server::AppState;
#[cfg(any(feature = "mcp-stdio", feature = "mcp-http", feature = "oauth"))]
use soma_runtime::server::AuthPolicy;
#[cfg(any(feature = "mcp-stdio", feature = "mcp-http"))]
use soma_runtime::server::gateway_product_state_from_env;
#[cfg(feature = "mcp-http")]
use soma_runtime::server::{AuthPolicyKind, resolve_auth_policy_kind};
#[cfg(any(
    feature = "mcp-stdio",
    feature = "mcp-http",
    all(
        any(test, feature = "test-support"),
        any(feature = "cli", feature = "mcp", feature = "api")
    )
))]
use soma_runtime::server::{GatewayProductState, SomaRuntime};
#[cfg(all(feature = "cli", feature = "mcp-stdio"))]
use tracing_subscriber::{EnvFilter, fmt};

#[path = "python_environment_operations.rs"]
mod python_environment_operations;
use python_environment_operations::python_environment_port;

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
    python_environment: Option<Arc<dyn PythonEnvironmentPort>>,
) -> Arc<SomaRuntime> {
    let mut ports = ApplicationPorts::unavailable()
        .with_gateway(Arc::new(soma_integrations::GatewayApplicationPort::new(
            gateway.clone(),
        )))
        .with_codemode(Arc::new(
            soma_integrations::CodeModeApplicationPort::default(),
        ));
    if let Some(python_environment) = python_environment {
        ports = ports.with_python_environment(python_environment);
    }
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

fn python_runner_selection(config: &Config) -> soma_application::PythonRunnerSelection {
    match config.python.mode {
        soma_config::PythonRunnerMode::OneShot => soma_application::PythonRunnerSelection::OneShot,
        soma_config::PythonRunnerMode::Persistent => {
            soma_application::PythonRunnerSelection::Persistent(
                soma_application::PythonSupervisorConfig {
                    startup_timeout: std::time::Duration::from_millis(
                        config.python.startup_timeout_ms,
                    ),
                    request_timeout: std::time::Duration::from_millis(
                        config.python.request_timeout_ms,
                    ),
                    shutdown_grace: std::time::Duration::from_millis(
                        config.python.shutdown_grace_ms,
                    ),
                    max_restarts: config.python.max_restarts,
                    restart_window: std::time::Duration::from_millis(
                        config.python.restart_window_ms,
                    ),
                    restart_backoff: std::time::Duration::from_millis(
                        config.python.restart_backoff_ms,
                    ),
                    max_stderr_bytes: config.python.max_stderr_bytes,
                    max_pending_bytes: config.python.max_pending_bytes,
                    max_workers: config.python.max_workers,
                    max_candidate_starts: config.python.max_candidate_starts,
                },
            )
        }
    }
}

fn python_provider_runtime(config: &Config) -> Result<soma_application::PythonProviderRuntime> {
    let runtime = soma_application::PythonProviderRuntime::new(python_runner_selection(config));
    let environment = &config.python.environment;
    if !environment.enabled {
        return Ok(runtime);
    }
    if environment.policy_version != soma_application::ENVIRONMENT_PLAN_VERSION {
        bail!(
            "unsupported Python environment policy version {}; this binary supports {}",
            environment.policy_version,
            soma_application::ENVIRONMENT_PLAN_VERSION
        );
    }
    let verified = verify_python_environment_inputs(environment)?;
    let fingerprint = soma_application::PythonRuntimeFingerprint::new(
        &environment.runtime_implementation,
        &environment.runtime_version,
        &environment.runtime_platform,
        &environment.wheel_platform_tag,
    )
    .map_err(|error| anyhow::anyhow!("invalid Python environment runtime identity: {error}"))?;
    let lifecycle = soma_application::PythonEnvironmentLifecycle::new(
        verified.uv_program,
        soma_application::PythonEnvironmentSpec {
            cache_root: verified.cache_root,
            runtime: fingerprint,
            python_executable: verified.python,
            sdk_wheel: verified.sdk_wheel,
            sdk_wheel_sha256: environment.sdk_wheel_sha256.clone(),
            uv_version: environment.uv_version.clone(),
            offline: environment.offline,
        },
    );
    Ok(
        runtime.with_environment_preparer(Arc::new(ConfiguredPythonEnvironmentPreparer {
            lifecycle,
            update: environment.update,
            environment: environment.clone(),
        })),
    )
}

struct ConfiguredPythonEnvironmentPreparer {
    lifecycle: soma_application::PythonEnvironmentLifecycle,
    update: bool,
    environment: soma_config::PythonEnvironmentConfig,
}

impl soma_application::PythonProviderEnvironmentPreparer for ConfiguredPythonEnvironmentPreparer {
    fn prepare(
        &self,
        provider_path: &std::path::Path,
    ) -> std::result::Result<soma_application::PythonInterpreter, String> {
        verify_python_environment_inputs(&self.environment).map_err(|error| error.to_string())?;
        let prepared = if self.update {
            self.lifecycle
                .update_provider(provider_path)
                .map(|report| report.candidate)
        } else {
            self.lifecycle.prepare_provider(provider_path)
        }
        .map_err(|error| error.to_string())?;
        verify_python_runtime(&prepared.python, &self.environment)
            .map_err(|error| error.to_string())?;
        Ok(soma_application::PythonInterpreter::prepared(&prepared))
    }

    fn validate_candidate(
        &self,
        provider_path: &std::path::Path,
        candidate: &soma_application::PreparedPythonEnvironment,
    ) -> std::result::Result<soma_application::PythonInterpreter, String> {
        verify_python_environment_inputs(&self.environment).map_err(|error| error.to_string())?;
        let prepared = self
            .lifecycle
            .validate_provider_candidate(provider_path, candidate)
            .map_err(|error| error.to_string())?;
        verify_python_runtime(&prepared.python, &self.environment)
            .map_err(|error| error.to_string())?;
        Ok(soma_application::PythonInterpreter::prepared(&prepared))
    }
}

struct VerifiedPythonEnvironmentInputs {
    cache_root: std::path::PathBuf,
    uv_program: std::path::PathBuf,
    python: std::path::PathBuf,
    sdk_wheel: std::path::PathBuf,
}

fn verify_python_environment_inputs(
    environment: &soma_config::PythonEnvironmentConfig,
) -> Result<VerifiedPythonEnvironmentInputs> {
    use sha2::{Digest, Sha256};

    let cache_root = std::path::PathBuf::from(&environment.cache_root);
    prepare_private_cache_root(&cache_root)?;
    let python = canonical_regular_file(
        &environment.python_executable,
        "Python environment interpreter",
    )?;
    let sdk_wheel = canonical_regular_file(&environment.sdk_wheel, "Python environment SDK wheel")?;
    let actual_digest = Sha256::digest(
        std::fs::read(&sdk_wheel)
            .with_context(|| format!("failed to read SDK wheel {}", sdk_wheel.display()))?,
    );
    let actual_digest = actual_digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if actual_digest != environment.sdk_wheel_sha256.to_ascii_lowercase() {
        bail!("Python environment SDK wheel SHA-256 does not match configured digest");
    }
    verify_python_runtime(&python, environment)?;
    verify_wheel_platform_tag(environment)?;

    let uv_program = std::path::PathBuf::from(&environment.uv_program);
    if !uv_program.is_absolute() {
        bail!("SOMA_PYTHON_ENVIRONMENT_UV_PROGRAM must be an absolute path");
    }
    Ok(VerifiedPythonEnvironmentInputs {
        cache_root,
        uv_program,
        python,
        sdk_wheel,
    })
}

fn canonical_regular_file(value: &str, label: &str) -> Result<std::path::PathBuf> {
    let path = std::path::Path::new(value);
    let canonical = path
        .canonicalize()
        .with_context(|| format!("{label} {} is unavailable", path.display()))?;
    if !canonical
        .metadata()
        .with_context(|| format!("failed to inspect {label} {}", canonical.display()))?
        .is_file()
    {
        bail!("{label} {} is not a regular file", canonical.display());
    }
    Ok(canonical)
}

fn prepare_private_cache_root(path: &std::path::Path) -> Result<()> {
    let existed = path.exists();
    std::fs::create_dir_all(path)
        .with_context(|| format!("failed to create Python cache root {}", path.display()))?;
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect Python cache root {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!(
            "Python environment cache root {} must be a real directory",
            path.display()
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        use std::os::unix::fs::PermissionsExt;
        if metadata.uid() != nix::unistd::Uid::effective().as_raw() {
            bail!(
                "Python environment cache root {} must be owned by the service user",
                path.display()
            );
        }
        if metadata.permissions().mode() & 0o077 != 0 {
            if existed {
                bail!(
                    "Python environment cache root {} must not grant group or other permissions",
                    path.display()
                );
            }
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).with_context(
                || {
                    format!(
                        "failed to make Python cache root {} private",
                        path.display()
                    )
                },
            )?;
        }
    }
    #[cfg(windows)]
    bail!(
        "managed Python environments require private-cache ACL enforcement, which is not yet supported on Windows"
    );
    #[cfg(unix)]
    Ok(())
}

fn verify_wheel_platform_tag(environment: &soma_config::PythonEnvironmentConfig) -> Result<()> {
    let platform = environment.runtime_platform.as_str();
    let tag = environment.wheel_platform_tag.as_str();
    let compatible = match platform.split_once('-') {
        Some(("linux", "x86_64")) => {
            tag.ends_with("_x86_64")
                && (tag.starts_with("manylinux_")
                    || tag.starts_with("musllinux_")
                    || tag == "linux_x86_64")
        }
        Some(("linux", "aarch64")) => {
            tag.ends_with("_aarch64")
                && (tag.starts_with("manylinux_")
                    || tag.starts_with("musllinux_")
                    || tag == "linux_aarch64")
        }
        Some(("windows", "x86_64")) => tag == "win_amd64",
        Some(("windows", "aarch64")) => tag == "win_arm64",
        Some(("macos", "x86_64")) => tag.starts_with("macosx_") && tag.ends_with("_x86_64"),
        Some(("macos", "aarch64")) => tag.starts_with("macosx_") && tag.ends_with("_arm64"),
        _ => false,
    };
    if !compatible {
        bail!(
            "Python wheel platform tag {:?} is incompatible with runtime platform {:?}",
            tag,
            platform
        );
    }
    Ok(())
}

fn verify_python_runtime(
    python: &std::path::Path,
    environment: &soma_config::PythonEnvironmentConfig,
) -> Result<()> {
    let probe = concat!(
        "import platform,sys\n",
        "system={'Darwin':'macos','Windows':'windows'}.get(platform.system(),",
        "platform.system().lower())\n",
        "machine={'AMD64':'x86_64','arm64':'aarch64'}.get(platform.machine(),",
        "platform.machine().lower())\n",
        "print(sys.implementation.name+'\\t'+platform.python_version()+'\\t'+system+'-'+machine)\n"
    );
    let output = std::process::Command::new(python)
        .args(["-I", "-c", probe])
        .output()
        .with_context(|| format!("failed to probe Python interpreter {}", python.display()))?;
    if !output.status.success() {
        bail!(
            "Python interpreter identity probe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let actual = String::from_utf8(output.stdout)
        .context("Python interpreter identity probe returned non-UTF-8 output")?;
    let expected = format!(
        "{}\t{}\t{}",
        environment.runtime_implementation,
        environment.runtime_version,
        environment.runtime_platform
    );
    if actual.trim() != expected {
        bail!(
            "Python interpreter identity mismatch: expected {expected:?}, got {:?}",
            actual.trim()
        );
    }
    Ok(())
}

#[cfg(feature = "cli")]
pub(crate) async fn cli_application_with_provider_dir(
    config: &Config,
    provider_dir: Option<&std::path::Path>,
) -> Result<Arc<SomaApplication>> {
    let service = SomaService::new(SomaClient::new(&config.soma)?);
    let registry = if config.soma.is_remote_adapter() {
        soma_application::remote_provider_registry(service.clone()).await?
    } else {
        let registry = match provider_dir {
            Some(provider_dir) => {
                soma_application::dynamic_provider_registry_from_dir_with_python_runtime(
                    service.clone(),
                    provider_dir,
                    python_provider_runtime(config)?,
                )
                .await?
            }
            None => {
                soma_application::dynamic_provider_registry_with_python_runtime(
                    service.clone(),
                    python_provider_runtime(config)?,
                )
                .await?
            }
        };
        registry
            .refresh_file_providers_async()
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        registry
    };
    let mut ports = ApplicationPorts::unavailable();
    if let Some(python_environment) = python_environment_port(config)? {
        ports = ports.with_python_environment(python_environment);
    }
    Ok(Arc::new(SomaApplication::new(
        Arc::new(service),
        Arc::new(registry),
        ports,
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
        soma_application::remote_provider_registry(service.clone()).await?
    } else {
        soma_application::dynamic_provider_registry_with_python_runtime(
            service.clone(),
            python_provider_runtime(&config)?,
        )
        .await?
    };
    let gateway = gateway_product_state_from_env()?;
    #[cfg(feature = "oauth")]
    configure_gateway_upstream_oauth_from_config(&gateway, &config.mcp.auth).await?;
    let runtime = runtime_for_components(
        service,
        provider_registry,
        gateway,
        python_environment_port(&config)?,
    );
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
    let provider_registry = soma_application::dynamic_provider_registry_with_python_runtime(
        service.clone(),
        python_provider_runtime(&config)?,
    )
    .await?;
    let gateway = gateway_product_state_from_env()?;
    #[cfg(feature = "oauth")]
    configure_gateway_upstream_oauth_for_policy(&gateway, &auth_policy, &config.mcp.auth).await?;
    let runtime = runtime_for_components(
        service,
        provider_registry,
        gateway,
        python_environment_port(&config)?,
    );
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
    auth: &soma_config::AuthConfig,
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
    let auth_config = soma_integrations::auth::soma_auth_config(auth)
        .map_err(|error| anyhow::anyhow!("Gateway upstream OAuth config error: {error}"))?;
    configure_gateway_upstream_oauth(gateway, &auth_config).await
}

#[cfg(all(feature = "oauth", feature = "mcp-stdio"))]
async fn configure_gateway_upstream_oauth_from_config(
    gateway: &soma_runtime::server::GatewayProductState,
    auth: &soma_config::AuthConfig,
) -> Result<()> {
    if !gateway_has_oauth_upstreams(gateway) {
        return Ok(());
    }
    let auth_config = soma_integrations::auth::soma_auth_config(auth)
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
            let auth_cfg = soma_integrations::auth::soma_auth_config(&config.mcp.auth)
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
