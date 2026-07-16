//! Client registration and redirect-URI resolution: RFC 7591 Dynamic Client
//! Registration (`POST /register`) and the redirect_uri trust boundary
//! shared by DCR-registered clients and CIMD `client_id`s (see
//! [`crate::cimd`]). Split out of `authorize.rs` to keep that module under
//! the repo's file-size contract — `authorize()` itself still lives there
//! and calls [`resolve_client_redirect_uris`] from here.

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::IntoResponse;
use axum::{Json, response::Response};
use tracing::{info, warn};

use crate::error::{AuthError, AuthErrorKind};
use crate::redirect_uri::is_allowed_redirect_uri;
use crate::state::AuthState;
use crate::types::{ClientRegistrationRequest, ClientRegistrationResponse, RegisteredClient};
use crate::util::{now_unix, random_token, remote_ip};

pub async fn register_client(
    State(state): State<AuthState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(request): Json<ClientRegistrationRequest>,
) -> Result<Json<ClientRegistrationResponse>, RegistrationError> {
    state.check_register_rate_limit(remote_ip(addr)).await?;
    if request.redirect_uris.is_empty() {
        warn!("oauth register rejected: no redirect URIs provided");
        return Err(
            AuthError::Validation("at least one redirect URI is required".to_string()).into(),
        );
    }
    let native_callback_endpoint = crate::metadata::native_callback_endpoint(&state);
    for redirect_uri in &request.redirect_uris {
        if redirect_uri != &native_callback_endpoint
            && !is_allowed_redirect_uri(redirect_uri, &state.config.allowed_client_redirect_uris)
        {
            warn!(
                redirect_uri = %redirect_uri,
                native_callback_endpoint = %native_callback_endpoint,
                allowed_patterns = ?state.config.allowed_client_redirect_uris,
                "oauth register rejected: redirect URI is not in the allowlist, native callback, or loopback set"
            );
            return Err(RegistrationError::InvalidRedirectUri(format!(
                "redirect URI `{redirect_uri}` must target a loopback host, match the native callback endpoint, or match an allowed redirect pattern"
            )));
        }
    }

    // RFC 7591 / OIDC application_type. Accept the two registered values and
    // default to "web" when omitted; reject anything else so misconfigured
    // clients fail loudly rather than silently registering an unknown type.
    let application_type = match request.application_type.as_deref() {
        None | Some("web") => "web".to_string(),
        Some("native") => "native".to_string(),
        Some(other) => {
            warn!(
                application_type = %other,
                "oauth register rejected: unsupported application_type"
            );
            return Err(RegistrationError::InvalidClientMetadata(format!(
                "application_type `{other}` is not supported; use `web` or `native`"
            )));
        }
    };

    let client = RegisteredClient {
        client_id: random_token(18)?,
        redirect_uris: request.redirect_uris,
        created_at: now_unix(),
    };
    state.store.register_client(client.clone()).await?;
    info!(
        client_id = %client.client_id,
        redirect_uri_count = client.redirect_uris.len(),
        redirect_uris = ?client.redirect_uris,
        "oauth client registration accepted"
    );
    Ok(Json(ClientRegistrationResponse {
        client_id: client.client_id,
        redirect_uris: client.redirect_uris,
        token_endpoint_auth_method: "none".to_string(),
        application_type,
    }))
}

/// RFC 7591 §3.2.2 requires `/register` errors to be reported as HTTP 400
/// with a `{"error": ..., "error_description": ...}` body using one of the
/// RFC's defined error codes — unlike the generic `AuthError` ->
/// `IntoResponse` impl in `error.rs`, which returns 422 with a
/// `{"kind", "message"}` body. This is `register_client`'s dedicated error
/// type, mirroring `TokenEndpointError` in `token.rs` for the `/token`
/// endpoint (RFC 6749 §5.2).
pub enum RegistrationError {
    /// A `redirect_uris` entry failed validation (RFC 7591 §3.2.2).
    InvalidRedirectUri(String),
    /// `application_type` (or another client-metadata field) failed
    /// validation.
    InvalidClientMetadata(String),
    /// Any other failure surfaced from shared auth infrastructure (rate
    /// limiting, storage). Status codes are preserved from `AuthError`'s own
    /// semantics, but the response body still uses the RFC 7591
    /// `error`/`error_description` shape for consistency within this
    /// endpoint's responses.
    Auth(AuthError),
}

impl From<AuthError> for RegistrationError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl RegistrationError {
    fn oauth_error(&self) -> &'static str {
        match self {
            Self::InvalidRedirectUri(_) => "invalid_redirect_uri",
            Self::InvalidClientMetadata(_) => "invalid_client_metadata",
            Self::Auth(AuthError::RateLimited { .. }) => "temporarily_unavailable",
            // No RFC 7591 error code maps cleanly onto the remaining
            // AuthError variants (rate limiting aside); `invalid_client_metadata`
            // is the closest registration-scoped fallback so every `/register`
            // response still carries an RFC-defined code.
            Self::Auth(_) => "invalid_client_metadata",
        }
    }

    fn log_kind(&self) -> &'static str {
        match self {
            Self::InvalidRedirectUri(_) => "invalid_redirect_uri",
            Self::InvalidClientMetadata(_) => "invalid_client_metadata",
            Self::Auth(error) => error.kind(),
        }
    }

    /// The two RFC 7591-specific variants always answer 400 per §3.2.2. The
    /// `Auth(_)` passthrough intentionally mirrors `AuthError`'s own private
    /// `status()` mapping in `error.rs` verbatim rather than introducing a
    /// registration-specific remap — the task for this endpoint is only to
    /// change the *body shape* for those errors (`error`/`error_description`
    /// instead of `kind`/`message`), not their existing status codes.
    fn status(&self) -> StatusCode {
        match self {
            Self::InvalidRedirectUri(_) | Self::InvalidClientMetadata(_) => StatusCode::BAD_REQUEST,
            Self::Auth(AuthError::InvalidGrant(_)) => StatusCode::BAD_REQUEST,
            Self::Auth(AuthError::AuthFailed(_) | AuthError::InvalidAccessToken) => {
                StatusCode::UNAUTHORIZED
            }
            Self::Auth(AuthError::Validation(_)) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Auth(AuthError::Network(_) | AuthError::Server(_)) => StatusCode::BAD_GATEWAY,
            Self::Auth(AuthError::RateLimited { .. }) => StatusCode::TOO_MANY_REQUESTS,
            Self::Auth(
                AuthError::Config(_)
                | AuthError::Storage(_)
                | AuthError::Decode(_)
                | AuthError::InsecurePermissions { .. },
            ) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn description(&self) -> String {
        match self {
            Self::InvalidRedirectUri(message) | Self::InvalidClientMetadata(message) => {
                message.clone()
            }
            Self::Auth(error) => error.to_string(),
        }
    }

    fn retry_after_ms(&self) -> Option<u64> {
        match self {
            Self::Auth(AuthError::RateLimited { retry_after_ms, .. }) => Some(*retry_after_ms),
            _ => None,
        }
    }
}

impl IntoResponse for RegistrationError {
    fn into_response(self) -> Response {
        let status = self.status();
        let log_kind = self.log_kind();
        let retry_after_ms = self.retry_after_ms();
        let body = Json(serde_json::json!({
            "error": self.oauth_error(),
            "error_description": self.description(),
        }));
        let mut response = (status, body).into_response();
        response.extensions_mut().insert(AuthErrorKind(log_kind));
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response
            .headers_mut()
            .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
        if let Some(retry_after_ms) = retry_after_ms
            && let Ok(value) = HeaderValue::from_str(&(retry_after_ms / 1_000).max(1).to_string())
        {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        response
    }
}

/// Filter `candidate_redirect_uris` down to those that pass the same
/// loopback/native-app-scheme/operator-allowlist check DCR-registered
/// clients are held to via [`is_allowed_redirect_uri`].
///
/// CIMD lets a client skip the DCR round-trip, not the redirect-URI trust
/// boundary. `client_id` is an arbitrary attacker-hosted URL, which means
/// the attacker also controls the JSON body served there — including
/// `redirect_uris`. Trusting a CIMD document's `redirect_uris` outright
/// would let any public HTTPS server declare
/// `redirect_uris: ["https://attacker.evil/steal-code"]` and have it
/// honored, making CIMD strictly weaker than DCR at exactly the point DCR
/// exists to protect. This function is a pure, dependency-free filter so
/// it's testable without any network/fetch involved.
pub(crate) fn allowlist_redirect_uris(
    candidate_redirect_uris: &[String],
    allowed_patterns: &[String],
) -> Vec<String> {
    candidate_redirect_uris
        .iter()
        .filter(|uri| is_allowed_redirect_uri(uri, allowed_patterns))
        .cloned()
        .collect()
}

/// Filter a fetched CIMD document's `redirect_uris` through
/// [`allowlist_redirect_uris`] and turn an empty result into the
/// appropriate rejection. Split out from [`resolve_client_redirect_uris`]
/// as a pure function (no fetch, no I/O) so this decision is unit-testable
/// directly: `resolve_client_redirect_uris` itself can only be exercised
/// end-to-end through a real CIMD fetch, which requires a public https host
/// this crate's test suite has no way to provide.
pub(crate) fn allowed_uris_from_cimd_document(
    document: &crate::cimd::document::ClientMetadataDocument,
    client_id: &str,
    client_state_id: &str,
    allowed_patterns: &[String],
) -> Result<Vec<String>, AuthError> {
    let allowed = allowlist_redirect_uris(&document.redirect_uris, allowed_patterns);
    if allowed.is_empty() {
        warn!(
            client_id = %client_id,
            client_state_id = %client_state_id,
            "oauth authorize rejected: CIMD document declares no allowlisted redirect_uris"
        );
        return Err(AuthError::Validation(
            "client_id metadata document declares no allowed redirect_uris".to_string(),
        ));
    }
    Ok(allowed)
}

/// Resolve the set of trusted `redirect_uris` for `client_id`, either via
/// the DCR-registered-clients table or, for an `https://`-shaped
/// `client_id`, by fetching and validating its CIMD document (see
/// [`crate::cimd`]) and filtering its declared `redirect_uris` through
/// [`allowed_uris_from_cimd_document`].
pub(crate) async fn resolve_client_redirect_uris(
    state: &AuthState,
    client_id: &str,
    client_state_id: &str,
) -> Result<Vec<String>, AuthError> {
    if crate::cimd::document::is_cimd_client_id(client_id) {
        let document =
            crate::cimd::document::fetch_and_validate_client_metadata(&state.cimd_cache, client_id)
                .await
                .map_err(|error| {
                    warn!(
                        client_id = %client_id,
                        client_state_id = %client_state_id,
                        kind = error.kind(),
                        error = %error,
                        "oauth authorize rejected: CIMD document fetch/validation failed"
                    );
                    // Deliberately generic: the detailed CimdError string (which can
                    // reveal e.g. "resolved only to private addresses" vs "does not
                    // exist") is logged above but NOT returned to the anonymous
                    // /authorize caller, to avoid an internal-network-topology
                    // mapping oracle.
                    AuthError::Validation(
                        "client_id metadata document is invalid or unreachable".to_string(),
                    )
                })?;
        return allowed_uris_from_cimd_document(
            &document,
            client_id,
            client_state_id,
            &state.config.allowed_client_redirect_uris,
        );
    }

    let client = state.store.find_client(client_id).await?.ok_or_else(|| {
        warn!(
            client_id = %client_id,
            client_state_id = %client_state_id,
            "oauth authorize rejected: unknown client_id"
        );
        AuthError::InvalidGrant("unknown client_id".to_string())
    })?;
    Ok(client.redirect_uris)
}
