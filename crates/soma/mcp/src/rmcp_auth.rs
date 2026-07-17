use rmcp::{service::RequestContext, ErrorData, RoleServer};
use serde_json::{json, Value};
use soma_domain::{AuthorizationMode, Principal, ScopeSet};

use super::state::{McpRouteScope, McpState};

#[cfg(feature = "auth")]
pub(super) use soma_auth::AuthContext;

#[cfg(not(feature = "auth"))]
pub(super) struct AuthContext {
    sub: String,
    scopes: Vec<String>,
}

pub(super) fn require_auth_context<'a>(
    state: &McpState,
    ctx: &'a RequestContext<RoleServer>,
) -> Result<Option<&'a AuthContext>, ErrorData> {
    match state.authorization_mode() {
        AuthorizationMode::LoopbackDev | AuthorizationMode::TrustedGateway => Ok(None),
        AuthorizationMode::Mounted => {
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

pub(super) fn principal(auth: Option<&AuthContext>) -> Principal {
    match auth {
        Some(auth) => authenticated_principal(auth),
        None => Principal::new(
            "loopback-dev",
            ScopeSet::from([soma_domain::actions::READ_SCOPE]),
        ),
    }
}

#[cfg(feature = "auth")]
fn authenticated_principal(auth: &AuthContext) -> Principal {
    Principal::new(auth.sub.clone(), ScopeSet::new(auth.scopes.clone()))
        .with_issuer(auth.issuer.clone())
}

#[cfg(not(feature = "auth"))]
fn authenticated_principal(auth: &AuthContext) -> Principal {
    Principal::new(auth.sub.clone(), ScopeSet::new(auth.scopes.clone()))
}

pub(super) fn protected_route_scope(ctx: &RequestContext<RoleServer>) -> Option<&McpRouteScope> {
    ctx.extensions
        .get::<http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<McpRouteScope>())
}

pub(super) fn protected_scope_allows_service(scope: Option<&McpRouteScope>, service: &str) -> bool {
    scope.is_none_or(|scope| scope.services.iter().any(|allowed| allowed == service))
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
