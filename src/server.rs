//! HTTP server application state and auth policy.
//!
//! `AppState` is injected into every request handler via axum's `State` extractor.
//! `AuthPolicy` determines which auth middleware (if any) is mounted on the router.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use anyhow::Result;

use crate::{
    app::ExampleService,
    config::{AuthMode, Config, McpConfig},
};

#[cfg(feature = "mcp-http")]
pub mod routes;

#[cfg(feature = "mcp-http")]
pub use routes::router;

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
        auth_state: Option<Arc<lab_auth::state::AuthState>>,
    },
}

impl std::fmt::Debug for AuthPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthPolicy::LoopbackDev => f.write_str("AuthPolicy::LoopbackDev"),
            AuthPolicy::TrustedGatewayUnscoped => f.write_str("AuthPolicy::TrustedGatewayUnscoped"),
            AuthPolicy::Mounted {
                auth_state: Some(_),
            } => f.write_str("AuthPolicy::Mounted { auth_state: Some(<AuthState>) }"),
            AuthPolicy::Mounted { auth_state: None } => {
                f.write_str("AuthPolicy::Mounted { auth_state: None /* bearer-only */ }")
            }
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

/// Read RTEMPLATE_NOAUTH from the environment directly.
///
/// Prefer `config.mcp.trusted_gateway` (loaded via `Config::load`) when a
/// typed config is available. This function exists for call sites that need the
/// value before config is fully loaded (e.g. early startup guards).
pub fn trusted_gateway_from_env() -> bool {
    std::env::var("RTEMPLATE_NOAUTH")
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
            "Refusing to bind MCP server to {} with RTEMPLATE_MCP_NO_AUTH=true.\n\
             \n\
             RTEMPLATE_MCP_NO_AUTH is only allowed on loopback binds. For a trusted \
             upstream proxy deployment, also set RTEMPLATE_NOAUTH=true.",
            config.mcp.host
        );
    }

    if has_oauth {
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
             1. Bind to loopback:    RTEMPLATE_MCP_HOST=127.0.0.1\n\
             2. Set a bearer token:  RTEMPLATE_MCP_TOKEN=$(openssl rand -hex 32)\n\
             3. Enable OAuth:        RTEMPLATE_MCP_AUTH_MODE=oauth (+ OAuth credentials)\n\
             4. Local no-auth dev:   RTEMPLATE_MCP_HOST=127.0.0.1 RTEMPLATE_MCP_NO_AUTH=true\n\
             5. Upstream gateway:    RTEMPLATE_NOAUTH=true  (if a proxy handles auth)\n\
             \n\
             TEMPLATE: Replace RTEMPLATE_ prefix with your service's prefix throughout.",
            config.mcp.host
        );
    }
}

fn validate_public_url(config: &Config) -> Result<()> {
    let Some(public_url) = config.mcp.auth.public_url.as_deref() else {
        return Ok(());
    };
    let parsed = url::Url::parse(public_url)
        .map_err(|error| anyhow::anyhow!("RTEMPLATE_MCP_PUBLIC_URL is invalid: {error}"))?;
    let Some(host) = parsed.host_str() else {
        anyhow::bail!("RTEMPLATE_MCP_PUBLIC_URL must include a host");
    };
    if host.contains('*') {
        anyhow::bail!("RTEMPLATE_MCP_PUBLIC_URL must not contain wildcard hosts");
    }
    Ok(())
}

/// Shared application state injected into every request handler.
#[derive(Clone)]
pub struct AppState {
    pub config: McpConfig,
    pub auth_policy: AuthPolicy,
    pub service: ExampleService,
    pub response_pages: ResponsePageStore,
}

#[derive(Clone, Default)]
pub struct ResponsePageStore {
    inner: Arc<ResponsePageStoreInner>,
}

#[derive(Default)]
struct ResponsePageStoreInner {
    counter: AtomicU64,
    entries: Mutex<HashMap<String, CachedResponsePage>>,
}

struct CachedResponsePage {
    serialized: String,
    expires_at: Instant,
}

impl ResponsePageStore {
    const TTL: Duration = Duration::from_secs(300);

    pub fn insert(&self, serialized: String) -> String {
        self.prune_expired();
        let id = self.inner.counter.fetch_add(1, Ordering::Relaxed) + 1;
        let cursor = format!("rsp_{id:x}");
        let entry = CachedResponsePage {
            serialized,
            expires_at: Instant::now() + Self::TTL,
        };
        self.inner
            .entries
            .lock()
            .expect("response page store mutex should not be poisoned")
            .insert(cursor.clone(), entry);
        cursor
    }

    pub fn get(&self, cursor: &str) -> Option<String> {
        self.prune_expired();
        self.inner
            .entries
            .lock()
            .expect("response page store mutex should not be poisoned")
            .get(cursor)
            .map(|entry| entry.serialized.clone())
    }

    fn prune_expired(&self) {
        let now = Instant::now();
        self.inner
            .entries
            .lock()
            .expect("response page store mutex should not be poisoned")
            .retain(|_, entry| entry.expires_at > now);
    }
}

/// Build an [`AuthLayer`] from an [`AuthPolicy`], or `None` when the trust
/// boundary is outside the mounted HTTP auth layer.
#[cfg(feature = "api")]
pub fn build_auth_layer(
    policy: &AuthPolicy,
    static_token: Option<Arc<str>>,
    resource_url: Option<Arc<str>>,
) -> Option<lab_auth::AuthLayer> {
    match policy {
        AuthPolicy::LoopbackDev | AuthPolicy::TrustedGatewayUnscoped => None,
        AuthPolicy::Mounted { auth_state } => {
            if static_token.is_none() && auth_state.is_none() {
                tracing::warn!(
                    "auth layer mounted but no static_token or auth_state configured — \
                     all requests will be rejected; set RTEMPLATE_MCP_TOKEN or configure OAuth"
                );
            }
            Some(
                lab_auth::AuthLayer::new()
                    .with_static_token(static_token)
                    .with_auth_state(auth_state.clone())
                    .with_static_token_scopes(vec![crate::actions::READ_SCOPE.into()])
                    .with_resource_url(resource_url)
                    .with_allow_session_cookie(false),
            )
        }
    }
}

#[cfg(test)]
#[path = "server_tests.rs"]
mod tests;
