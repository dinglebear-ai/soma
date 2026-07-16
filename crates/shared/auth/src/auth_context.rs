//! Auth context injected into request extensions by [`crate::middleware::AuthLayer`].
//!
//! Downstream handlers can read this when they need caller identity or scope
//! checks, but not every route consumes it yet.

use axum::http::request::Parts;
use std::sync::Arc;

/// Stored in request extensions by the HTTP auth middleware (see
/// [`crate::middleware::AuthLayer`]).
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// JWT `sub` claim (or `"static-bearer"` for static-token requests).
    pub sub: String,
    /// Optional opaque actor key (lab-specific observability hook); produced
    /// by the [`crate::middleware::ActorKeyDeriver`] closure when one is
    /// installed on the layer. Consumers without an actor-key concept
    /// (cortex etc.) leave this `None`.
    pub actor_key: Option<Arc<str>>,
    /// Effective scopes for this request.
    pub scopes: Vec<String>,
    /// JWT `iss` claim (or `"local"` / `"browser-session"` sentinel).
    pub issuer: String,
    /// `true` when the request was authenticated via the browser session
    /// cookie rather than a bearer token.
    pub via_session: bool,
    /// Browser-session CSRF token, when the request was authenticated via
    /// session cookie. Echoed back to handlers that need to mint a fresh
    /// `x-csrf-token` for follow-up state-changing requests.
    pub csrf_token: Option<String>,
    /// Verified Google email tied to the browser session, when known.
    pub email: Option<String>,
}

/// Build the value of an `WWW-Authenticate: Bearer ...` response header
/// pointing browsers/agents at the protected-resource metadata document.
///
/// When `scope` is `Some` and non-empty, a `scope="..."` parameter (RFC 6750
/// Section 3) is appended so clients get immediate guidance on which scopes
/// to request during authorization, per the MCP spec's `WWW-Authenticate`
/// guidance. `scope` is expected to be a space-joined scope list (e.g.
/// `"syslog:read syslog:admin"`).
#[must_use]
pub fn www_authenticate_value(resource_url: &str, scope: Option<&str>) -> String {
    let mut value = format!(
        "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\"",
        resource_url.trim_end_matches('/')
    );
    if let Some(scope) = scope
        && !scope.is_empty()
    {
        value.push_str(&format!(", scope=\"{scope}\""));
    }
    value
}

/// Convenience accessor for handlers that have already split a request into
/// [`Parts`].
#[must_use]
pub fn auth_context(parts: &Parts) -> Option<&AuthContext> {
    parts.extensions.get::<AuthContext>()
}

#[cfg(test)]
mod tests {
    use super::www_authenticate_value;

    #[test]
    fn www_authenticate_value_appends_metadata_path_and_strips_trailing_slash() {
        assert_eq!(
            www_authenticate_value("https://lab.example.com/", None),
            "Bearer resource_metadata=\"https://lab.example.com/.well-known/oauth-protected-resource\""
        );
        assert_eq!(
            www_authenticate_value("https://lab.example.com", None),
            "Bearer resource_metadata=\"https://lab.example.com/.well-known/oauth-protected-resource\""
        );
    }

    #[test]
    fn www_authenticate_value_appends_scope_when_present_and_omits_when_absent() {
        assert_eq!(
            www_authenticate_value("https://lab.example.com", Some("syslog:read syslog:admin")),
            "Bearer resource_metadata=\"https://lab.example.com/.well-known/oauth-protected-resource\", scope=\"syslog:read syslog:admin\""
        );
        // `None` and empty-string scopes are both treated as "nothing to
        // offer" and must not append a `scope=` param.
        assert_eq!(
            www_authenticate_value("https://lab.example.com", None),
            "Bearer resource_metadata=\"https://lab.example.com/.well-known/oauth-protected-resource\""
        );
        assert_eq!(
            www_authenticate_value("https://lab.example.com", Some("")),
            "Bearer resource_metadata=\"https://lab.example.com/.well-known/oauth-protected-resource\""
        );
    }
}
