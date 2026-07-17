//! HTTP server application state and auth policy.
//!
//! `AppState` is injected into every request handler via axum's `State` extractor.
//! `AuthPolicy` determines which auth middleware (if any) is mounted on the router.

use std::sync::Arc;

use anyhow::Result;

use soma_application::SomaApplication;
use soma_config::{AuthMode, Config, McpConfig};
#[cfg(feature = "protected-routes")]
use soma_gateway::config::ProtectedMcpRouteConfig;
use soma_gateway::{
    config::{GatewayConfig, GatewayPaths, UpstreamConfig},
    gateway::{config_store::FsGatewayConfigStore, manager::GatewayManager},
};
pub use soma_mcp_server::ResponsePageStore;

pub type GatewayProductState = Arc<GatewayManager>;

pub fn gateway_product_state_from_config(config: GatewayConfig) -> Result<GatewayProductState> {
    Ok(Arc::new(GatewayManager::new(config)?))
}

pub fn gateway_product_state_from_env() -> Result<GatewayProductState> {
    let paths = if std::env::var_os("MCP_GATEWAY_HOME").is_none() {
        match std::env::var_os("SOMA_HOME") {
            Some(home) => GatewayPaths::new(std::path::PathBuf::from(home).join(".mcp-gateway"))?,
            None => GatewayPaths::from_env()?,
        }
    } else {
        GatewayPaths::from_env()?
    };
    gateway_product_state_from_store(FsGatewayConfigStore::from_paths(paths))
}

pub fn gateway_product_state_from_store(
    store: FsGatewayConfigStore,
) -> Result<GatewayProductState> {
    Ok(Arc::new(GatewayManager::from_store(store)?))
}

#[must_use]
pub fn empty_gateway_product_state() -> GatewayProductState {
    gateway_product_state_from_config(GatewayConfig::default())
        .expect("empty gateway config should build")
}

/// Authentication policy attached to [`AppState`].
///
/// Intentionally an enum — constructing `AppState` requires an explicit choice.
/// There is no `Default` impl.
#[derive(Clone)]
pub enum AuthPolicy {
    /// No authentication. Only legal when bound to a loopback address.
    /// Scope checks are bypassed — the bind itself is the trust boundary.
    LoopbackDev,
    /// No local authentication or scope checks. The deployment must enforce
    /// both authentication and authorization before traffic reaches this server.
    TrustedGatewayUnscoped,
    /// Authentication middleware is mounted. Scope checks MUST run.
    /// - `Some(auth_state)`: OAuth mode (Google flow + JWKS issuance)
    /// - `None`: static bearer token only
    Mounted {
        #[cfg(feature = "auth")]
        auth_state: Option<Arc<soma_auth::state::AuthState>>,
    },
}

impl std::fmt::Debug for AuthPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthPolicy::LoopbackDev => f.write_str("AuthPolicy::LoopbackDev"),
            AuthPolicy::TrustedGatewayUnscoped => f.write_str("AuthPolicy::TrustedGatewayUnscoped"),
            #[cfg(feature = "auth")]
            AuthPolicy::Mounted {
                auth_state: Some(_),
            } => f.write_str("AuthPolicy::Mounted { auth_state: Some(<AuthState>) }"),
            #[cfg(feature = "auth")]
            AuthPolicy::Mounted { auth_state: None } => {
                f.write_str("AuthPolicy::Mounted { auth_state: None /* bearer-only */ }")
            }
            #[cfg(not(feature = "auth"))]
            AuthPolicy::Mounted {} => f.write_str("AuthPolicy::Mounted"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthPolicyKind {
    LoopbackDev,
    TrustedGatewayUnscoped,
    MountedBearer,
    MountedOAuth,
}

/// Read SOMA_NOAUTH from the environment directly.
///
/// Prefer `config.mcp.trusted_gateway` (loaded via `Config::load`) when a
/// typed config is available. This function exists for call sites that need the
/// value before config is fully loaded (e.g. early startup guards).
pub fn trusted_gateway_from_env() -> bool {
    std::env::var("SOMA_NOAUTH")
        .map(|v| matches!(v.to_lowercase().as_str(), "true" | "1" | "yes"))
        .unwrap_or(false)
}

pub fn resolve_auth_policy_kind(config: &Config, trusted_gateway: bool) -> Result<AuthPolicyKind> {
    validate_public_url(config)?;

    if config.mcp.is_loopback() {
        return Ok(AuthPolicyKind::LoopbackDev);
    }

    let has_token = config
        .mcp
        .api_token
        .as_deref()
        .map(|token| !token.is_empty())
        .unwrap_or(false);
    let has_oauth = config.mcp.auth.mode == AuthMode::OAuth;

    if config.mcp.no_auth {
        if trusted_gateway {
            return Ok(AuthPolicyKind::TrustedGatewayUnscoped);
        }
        anyhow::bail!(
            "Refusing to bind MCP server to {} with SOMA_MCP_NO_AUTH=true.\n\
             \n\
             SOMA_MCP_NO_AUTH is only allowed on loopback binds. For a trusted \
             upstream proxy deployment, also set SOMA_NOAUTH=true.",
            config.mcp.host
        );
    }

    if has_oauth {
        #[cfg(not(feature = "auth"))]
        anyhow::bail!(
            "SOMA_MCP_AUTH_MODE=oauth requires compiling with the `auth`/`oauth` feature"
        );
        #[cfg(feature = "auth")]
        Ok(AuthPolicyKind::MountedOAuth)
    } else if has_token {
        Ok(AuthPolicyKind::MountedBearer)
    } else if trusted_gateway {
        Ok(AuthPolicyKind::TrustedGatewayUnscoped)
    } else {
        anyhow::bail!(
            "Refusing to bind MCP server to {} without authentication.\n\
             \n\
             Choose one of:\n\
             1. Bind to loopback:    SOMA_MCP_HOST=127.0.0.1\n\
             2. Set a bearer token:  SOMA_MCP_TOKEN=$(openssl rand -hex 32)\n\
             3. Enable OAuth:        SOMA_MCP_AUTH_MODE=oauth (+ OAuth credentials)\n\
             4. Local no-auth dev:   SOMA_MCP_HOST=127.0.0.1 SOMA_MCP_NO_AUTH=true\n\
             5. Upstream gateway:    SOMA_NOAUTH=true  (if a proxy handles auth)\n\
             \n\
             CUSTOMIZE: Replace SOMA_ prefix with your service's prefix throughout.",
            config.mcp.host
        );
    }
}

fn validate_public_url(config: &Config) -> Result<()> {
    let Some(public_url) = config.mcp.auth.public_url.as_deref() else {
        return Ok(());
    };
    let parsed = url::Url::parse(public_url)
        .map_err(|error| anyhow::anyhow!("SOMA_MCP_PUBLIC_URL is invalid: {error}"))?;
    let Some(host) = parsed.host_str() else {
        anyhow::bail!("SOMA_MCP_PUBLIC_URL must include a host");
    };
    if host.contains('*') {
        anyhow::bail!("SOMA_MCP_PUBLIC_URL must not contain wildcard hosts");
    }
    Ok(())
}

/// Shared application state injected into every request handler.
#[derive(Clone)]
pub struct SomaRuntime {
    application: Arc<SomaApplication>,
    gateway: GatewayProductState,
}

impl SomaRuntime {
    pub fn new(application: Arc<SomaApplication>, gateway: GatewayProductState) -> Self {
        Self {
            application,
            gateway,
        }
    }

    pub fn application(&self) -> &SomaApplication {
        self.application.as_ref()
    }

    pub fn application_handle(&self) -> Arc<SomaApplication> {
        self.application.clone()
    }

    #[cfg(feature = "protected-routes")]
    pub fn resolve_protected_route(
        &self,
        host: &str,
        path: &str,
    ) -> Option<ProtectedMcpRouteConfig> {
        self.gateway.resolve_protected_route(host, path)
    }

    #[cfg(feature = "protected-routes")]
    pub fn resolve_protected_route_metadata(
        &self,
        host: &str,
        path: &str,
    ) -> Option<ProtectedMcpRouteConfig> {
        self.gateway.resolve_protected_route_metadata(host, path)
    }

    #[cfg(feature = "protected-routes")]
    pub fn protected_route_list(&self) -> Vec<ProtectedMcpRouteConfig> {
        self.gateway.protected_route_list()
    }

    pub fn upstream_config(&self, name: &str) -> Option<UpstreamConfig> {
        self.gateway.upstream_config(name)
    }

    #[cfg(feature = "oauth")]
    pub async fn upstream_oauth_access_token(
        &self,
        upstream: &UpstreamConfig,
        subject: &str,
    ) -> Result<Option<String>, soma_gateway::gateway::manager::GatewayManagerError> {
        self.gateway
            .upstream_oauth_access_token(upstream, subject)
            .await
    }
}

/// Shared transport state injected into every request handler.
#[derive(Clone)]
pub struct AppState {
    pub config: McpConfig,
    pub auth_policy: AuthPolicy,
    runtime: Arc<SomaRuntime>,
    pub response_pages: ResponsePageStore,
}

impl AppState {
    pub fn new(
        config: McpConfig,
        auth_policy: AuthPolicy,
        runtime: Arc<SomaRuntime>,
        response_pages: ResponsePageStore,
    ) -> Self {
        Self {
            config,
            auth_policy,
            runtime,
            response_pages,
        }
    }

    pub fn runtime(&self) -> &SomaRuntime {
        self.runtime.as_ref()
    }

    pub fn application(&self) -> &SomaApplication {
        self.runtime.application()
    }

    pub fn application_handle(&self) -> Arc<SomaApplication> {
        self.runtime.application_handle()
    }

    #[cfg(feature = "protected-routes")]
    pub fn resolve_protected_route(
        &self,
        host: &str,
        path: &str,
    ) -> Option<ProtectedMcpRouteConfig> {
        self.runtime.resolve_protected_route(host, path)
    }

    #[cfg(feature = "protected-routes")]
    pub fn resolve_protected_route_metadata(
        &self,
        host: &str,
        path: &str,
    ) -> Option<ProtectedMcpRouteConfig> {
        self.runtime.resolve_protected_route_metadata(host, path)
    }

    #[cfg(feature = "protected-routes")]
    pub fn protected_route_list(&self) -> Vec<ProtectedMcpRouteConfig> {
        self.runtime.protected_route_list()
    }

    pub fn upstream_config(&self, name: &str) -> Option<UpstreamConfig> {
        self.runtime.upstream_config(name)
    }

    #[cfg(feature = "oauth")]
    pub async fn upstream_oauth_access_token(
        &self,
        upstream: &UpstreamConfig,
        subject: &str,
    ) -> Result<Option<String>, soma_gateway::gateway::manager::GatewayManagerError> {
        self.runtime
            .upstream_oauth_access_token(upstream, subject)
            .await
    }
}

/// Build an [`AuthLayer`] from an [`AuthPolicy`], or `None` when the trust
/// boundary is outside the mounted HTTP auth layer.
#[cfg(feature = "auth")]
pub fn build_auth_layer(
    policy: &AuthPolicy,
    static_token: Option<Arc<str>>,
    resource_url: Option<Arc<str>>,
) -> Option<soma_auth::AuthLayer> {
    match policy {
        AuthPolicy::LoopbackDev | AuthPolicy::TrustedGatewayUnscoped => None,
        AuthPolicy::Mounted { auth_state } => {
            if static_token.is_none() && auth_state.is_none() {
                tracing::warn!(
                    "auth layer mounted but no static_token or auth_state configured — \
                     all requests will be rejected; set SOMA_MCP_TOKEN or configure OAuth"
                );
            }
            Some(
                soma_auth::AuthLayer::new()
                    .with_static_token(static_token)
                    .with_auth_state(auth_state.clone())
                    .with_static_token_scopes(vec![soma_domain::actions::READ_SCOPE.into()])
                    .with_resource_url(resource_url)
                    .with_allow_session_cookie(false),
            )
        }
    }
}

#[cfg(not(feature = "auth"))]
pub fn build_auth_layer(
    _policy: &AuthPolicy,
    _static_token: Option<Arc<str>>,
    _resource_url: Option<Arc<str>>,
) -> Option<()> {
    None
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
