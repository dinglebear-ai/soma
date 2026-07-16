use std::borrow::Cow;

use rmcp::{service::RequestContext, ErrorData, RoleServer};
use serde_json::{json, Value};
use soma_gateway::gateway::protected_routes::ProtectedRouteScope;
use soma_runtime::server::{AppState, AuthPolicy};
use soma_service::{ProviderAuthMode, ProviderPrincipal};

#[cfg(feature = "auth")]
pub(super) use soma_auth::AuthContext;

#[cfg(not(feature = "auth"))]
pub(super) struct AuthContext {
    sub: String,
    scopes: Vec<String>,
}

pub(super) fn require_auth_context<'a>(
    state: &AppState,
    ctx: &'a RequestContext<RoleServer>,
) -> Result<Option<&'a AuthContext>, ErrorData> {
    match &state.auth_policy {
        AuthPolicy::LoopbackDev | AuthPolicy::TrustedGatewayUnscoped => Ok(None),
        AuthPolicy::Mounted { .. } => {
            let parts = ctx
                .extensions
                .get::<http::request::Parts>()
                .ok_or_else(|| {
                    tracing::error!(
                        "rmcp HTTP Parts extension absent - middleware ordering may be broken"
                    );
                    ErrorData::invalid_request(
                        "forbidden: missing http context",
                        Some(auth_protocol_error_payload(
                            "missing_http_context",
                            "MCP HTTP request context was unavailable for auth enforcement.",
                            "Check RMCP router mounting and middleware ordering. HTTP transports must preserve request Parts extensions before auth is enforced.",
                        )),
                    )
                })?;
            let auth = parts.extensions.get::<AuthContext>().ok_or_else(|| {
                tracing::warn!("AuthContext absent - AuthLayer may not be mounted");
                ErrorData::invalid_request(
                    "forbidden: missing auth context",
                    Some(auth_protocol_error_payload(
                        "missing_auth_context",
                        "MCP auth context was unavailable for this request.",
                        "Reconnect with a valid bearer token or OAuth session, and verify AuthLayer is mounted for the MCP route.",
                    )),
                )
            })?;
            Ok(Some(auth))
        }
    }
}

pub(super) fn provider_principal(auth: Option<&AuthContext>) -> ProviderPrincipal {
    match auth {
        Some(auth) => ProviderPrincipal {
            subject: auth.sub.clone(),
            scopes: auth.scopes.clone(),
        },
        None => ProviderPrincipal::loopback_dev(),
    }
}

pub(super) fn provider_auth_mode(policy: &AuthPolicy) -> ProviderAuthMode {
    match policy {
        AuthPolicy::LoopbackDev => ProviderAuthMode::LoopbackDev,
        AuthPolicy::TrustedGatewayUnscoped => ProviderAuthMode::TrustedGateway,
        AuthPolicy::Mounted { .. } => ProviderAuthMode::Mounted,
    }
}

pub(super) fn gateway_oauth_subject(auth: Option<&AuthContext>) -> Cow<'_, str> {
    const SHARED_GATEWAY_OAUTH_SUBJECT: &str = "gateway";
    match auth {
        None => Cow::Borrowed(SHARED_GATEWAY_OAUTH_SUBJECT),
        Some(auth)
            if auth_context_is_local(auth)
                || soma_contracts::scopes::has_admin_scope(&auth.scopes) =>
        {
            Cow::Borrowed(SHARED_GATEWAY_OAUTH_SUBJECT)
        }
        Some(auth) => Cow::Borrowed(auth.sub.as_str()),
    }
}

pub(super) fn protected_route_scope(
    ctx: &RequestContext<RoleServer>,
) -> Option<&ProtectedRouteScope> {
    ctx.extensions
        .get::<http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<ProtectedRouteScope>())
}

pub(super) fn protected_scope_allows_service(
    scope: Option<&ProtectedRouteScope>,
    service: &str,
) -> bool {
    scope.is_none_or(|scope| scope.services.iter().any(|allowed| allowed == service))
}

#[cfg(feature = "auth")]
fn auth_context_is_local(auth: &AuthContext) -> bool {
    auth.issuer == "local"
}

#[cfg(not(feature = "auth"))]
fn auth_context_is_local(_auth: &AuthContext) -> bool {
    false
}

fn auth_protocol_error_payload(
    code: &str,
    message: impl Into<String>,
    remediation: impl Into<String>,
) -> Value {
    json!({
        "kind": "mcp_auth_error",
        "schema_version": 1,
        "code": code,
        "message": message.into(),
        "retryable": false,
        "remediation": remediation.into(),
    })
}

#[cfg(test)]
#[path = "rmcp_auth_tests.rs"]
mod tests;
