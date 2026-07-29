//! Client registration and redirect-URI resolution: RFC 7591 Dynamic Client
//! Registration (`POST /register`) and the redirect_uri trust boundary
//! shared by DCR-registered clients and CIMD `client_id`s (see
//! [`crate::cimd`]). Split out of `authorize.rs` to keep that module under
//! the repo's file-size contract — `authorize()` itself still lives there
//! and calls `resolve_client_redirect_uris` from here.

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, response::Response};
use tracing::{info, warn};

use crate::error::AuthError;
use crate::redirect_uri::is_allowed_redirect_uri;
use crate::state::AuthState;
use crate::types::{ClientRegistrationRequest, ClientRegistrationResponse, RegisteredClient};
use crate::util::{now_unix, oauth_error_response, random_token, remote_ip};

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
        token_endpoint_auth_method: "none".to_string(),
        jwks: None,
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
            Self::Auth(AuthError::InvalidGrant(_) | AuthError::InvalidScope(_)) => {
                StatusCode::BAD_REQUEST
            }
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
        oauth_error_response(
            self.status(),
            self.oauth_error(),
            self.description(),
            self.log_kind(),
            self.retry_after_ms(),
        )
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

/// Where a `client_id` was resolved from, carrying that source's own
/// payload.
///
/// This is the single place the CIMD-vs-DCR-store decision is made (see
/// [`resolve_client_source`]). The two public resolvers — [`resolve_client`]
/// for `/token` and [`resolve_client_redirect_uris`] for `/authorize` — both
/// branch on this enum instead of re-testing
/// [`crate::cimd::document::is_cimd_client_id`] themselves, so the two
/// endpoints can never disagree about whether a given `client_id` resolves.
///
/// The CIMD variant deliberately carries the raw
/// [`crate::cimd::document::ClientMetadataDocument`] rather than an already
/// converted [`RegisteredClient`]: `/authorize` must run the document's
/// `redirect_uris` through [`allowed_uris_from_cimd_document`], which is a
/// pure, separately unit-tested function over the document itself.
enum ResolvedClientSource {
    /// `client_id` is a CIMD URL and its metadata document was fetched and
    /// validated.
    Cimd(crate::cimd::document::ClientMetadataDocument),
    /// `client_id` is an opaque DCR-issued token; `None` when the clients
    /// table has no such row. Turning that `None` into a caller-appropriate
    /// answer is each resolver's own job — `/token` treats it as `Ok(None)`,
    /// `/authorize` as `Err(InvalidGrant)`.
    Registered(Option<RegisteredClient>),
}

/// The shared CIMD-vs-store branch behind [`resolve_client`] and
/// [`resolve_client_redirect_uris`].
///
/// `on_cimd_error` runs before the `CimdError` is collapsed into the
/// deliberately generic [`AuthError::Validation`] both callers return, so a
/// caller that wants the detailed failure in its logs (the `/authorize` path
/// does; the `/token` path does not) can record it without the detail ever
/// reaching the anonymous HTTP caller.
async fn resolve_client_source(
    state: &AuthState,
    client_id: &str,
    on_cimd_error: impl FnOnce(&crate::cimd::document::CimdError),
) -> Result<ResolvedClientSource, AuthError> {
    if crate::cimd::document::is_cimd_client_id(client_id) {
        let document =
            crate::cimd::document::fetch_and_validate_client_metadata(&state.cimd_cache, client_id)
                .await
                .map_err(|error| {
                    on_cimd_error(&error);
                    // Deliberately generic: the detailed CimdError string (which can
                    // reveal e.g. "resolved only to private addresses" vs "does not
                    // exist") is only ever exposed through `on_cimd_error`, NOT
                    // returned to the anonymous caller, to avoid an
                    // internal-network-topology mapping oracle.
                    AuthError::Validation(
                        "client_id metadata document is invalid or unreachable".to_string(),
                    )
                })?;
        return Ok(ResolvedClientSource::Cimd(document));
    }
    Ok(ResolvedClientSource::Registered(
        state.store.find_client(client_id).await?,
    ))
}

/// Resolve complete client authentication metadata from DCR or CIMD.
///
/// Consumed by `token_client_auth::authenticate_oauth_client` to decide which
/// `token_endpoint_auth_method` a client must satisfy at `/token`.
///
/// An unknown `client_id` is `Ok(None)`, not an error: `/token`'s client
/// authentication has its own not-found handling and error shape (RFC 6749
/// §5.2), unlike `/authorize` — see [`resolve_client_redirect_uris`].
pub(crate) async fn resolve_client(
    state: &AuthState,
    client_id: &str,
) -> Result<Option<RegisteredClient>, AuthError> {
    // No logging hook: unlike /authorize, this path deliberately stays quiet
    // about CIMD fetch failures.
    match resolve_client_source(state, client_id, |_| {}).await? {
        ResolvedClientSource::Cimd(document) => Ok(Some(RegisteredClient {
            client_id: document.client_id,
            redirect_uris: document.redirect_uris,
            created_at: 0,
            token_endpoint_auth_method: document.token_endpoint_auth_method,
            jwks: document.jwks,
        })),
        ResolvedClientSource::Registered(client) => Ok(client),
    }
}

/// Resolve the set of trusted `redirect_uris` for `client_id`, either via
/// the DCR-registered-clients table or, for an `https://`-shaped
/// `client_id`, by fetching and validating its CIMD document (see
/// [`crate::cimd`]) and filtering its declared `redirect_uris` through
/// [`allowed_uris_from_cimd_document`].
///
/// Shares [`resolve_client_source`] with [`resolve_client`] so both
/// endpoints agree on whether a `client_id` resolves at all; what differs
/// is only what each does afterwards. Here an unknown `client_id` is a
/// logged [`AuthError::InvalidGrant`] rather than `Ok(None)`, because
/// `/authorize` has no later step that could handle "unknown".
pub(crate) async fn resolve_client_redirect_uris(
    state: &AuthState,
    client_id: &str,
    client_state_id: &str,
) -> Result<Vec<String>, AuthError> {
    let source = resolve_client_source(state, client_id, |error| {
        warn!(
            client_id = %client_id,
            client_state_id = %client_state_id,
            kind = error.kind(),
            error = %error,
            "oauth authorize rejected: CIMD document fetch/validation failed"
        );
    })
    .await?;

    match source {
        ResolvedClientSource::Cimd(document) => allowed_uris_from_cimd_document(
            &document,
            client_id,
            client_state_id,
            &state.config.allowed_client_redirect_uris,
        ),
        ResolvedClientSource::Registered(Some(client)) => Ok(client.redirect_uris),
        ResolvedClientSource::Registered(None) => {
            warn!(
                client_id = %client_id,
                client_state_id = %client_state_id,
                "oauth authorize rejected: unknown client_id"
            );
            Err(AuthError::InvalidGrant("unknown client_id".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tempfile::tempdir;
    use url::Url;

    use super::*;
    use crate::config::{AuthConfig, AuthMode, GoogleConfig};

    /// A DCR-issued `client_id` is an opaque `random_token(18)` value, so it
    /// can never start with `https://` and always takes the store branch.
    const OPAQUE_CLIENT_ID: &str = "opaque-dcr-client-id";
    /// An `https://` `client_id` always takes the CIMD branch. This one is
    /// rejected by `validate_url_shape`'s private-address guard before any
    /// DNS or network I/O happens, so the test stays hermetic.
    const CIMD_CLIENT_ID: &str = "https://127.0.0.1/client-metadata.json";

    async fn test_state() -> AuthState {
        let dir = Box::leak(Box::new(tempdir().expect("tempdir")));
        AuthState::new(AuthConfig {
            mode: AuthMode::OAuth,
            public_url: Some(Url::parse("https://lab.example.com").expect("url")),
            sqlite_path: dir.path().join("auth.db"),
            key_path: dir.path().join("auth.pem"),
            bootstrap_secret: None,
            allowed_client_redirect_uris: Vec::new(),
            admin_email: "user@example.com".to_string(),
            google: GoogleConfig {
                client_id: "client-id".to_string(),
                client_secret: "client-secret".to_string(),
                callback_path: "/auth/google/callback".to_string(),
                scopes: vec![
                    "openid".to_string(),
                    "email".to_string(),
                    "profile".to_string(),
                ],
            },
            access_token_ttl: Duration::from_secs(3600),
            refresh_token_ttl: Duration::from_secs(3600),
            auth_code_ttl: Duration::from_secs(300),
            register_requests_per_minute: 10,
            authorize_requests_per_minute: 20,
            max_pending_oauth_states: 1024,
            default_provider: "google".to_string(),
            ..AuthConfig::default()
        })
        .await
        .expect("auth state")
    }

    /// The regression this pairing exists to prevent: `/token` resolves a
    /// client through `resolve_client` and `/authorize` through
    /// `resolve_client_redirect_uris`. If those two ever branch differently
    /// on CIMD-vs-store, a `client_id` could authenticate at one endpoint
    /// and be unknown at the other. Both now share
    /// `resolve_client_source`, so assert they answer the same
    /// resolves/does-not-resolve verdict for the same `client_id` -- while
    /// still expressing that verdict in each endpoint's own shape.
    #[tokio::test]
    async fn both_resolvers_agree_a_registered_client_resolves() {
        let state = test_state().await;
        let registered = RegisteredClient {
            client_id: OPAQUE_CLIENT_ID.to_string(),
            redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
            created_at: 0,
            token_endpoint_auth_method: "none".to_string(),
            jwks: None,
        };
        state
            .store
            .register_client(registered.clone())
            .await
            .expect("register client");

        let client = resolve_client(&state, OPAQUE_CLIENT_ID)
            .await
            .expect("resolve_client")
            .expect("registered client resolves at /token");
        let redirect_uris = resolve_client_redirect_uris(&state, OPAQUE_CLIENT_ID, "state-id")
            .await
            .expect("registered client resolves at /authorize");

        assert_eq!(client.client_id, OPAQUE_CLIENT_ID);
        assert_eq!(client.redirect_uris, registered.redirect_uris);
        assert_eq!(redirect_uris, registered.redirect_uris);
    }

    #[tokio::test]
    async fn both_resolvers_agree_an_unknown_client_does_not_resolve() {
        let state = test_state().await;

        // /token: unknown is `Ok(None)`, left for client authentication to
        // report in RFC 6749 section 5.2 shape.
        let token_side = resolve_client(&state, OPAQUE_CLIENT_ID)
            .await
            .expect("resolve_client does not error on unknown clients");
        assert!(token_side.is_none());

        // /authorize: the same verdict, but reported as InvalidGrant because
        // there is no later step that could handle "unknown".
        let authorize_side = resolve_client_redirect_uris(&state, OPAQUE_CLIENT_ID, "state-id")
            .await
            .expect_err("unknown client_id must fail /authorize");
        match authorize_side {
            AuthError::InvalidGrant(message) => assert_eq!(message, "unknown client_id"),
            other => panic!("expected InvalidGrant, got {other:?}"),
        }
    }

    /// Both resolvers must route an `https://` `client_id` down the CIMD
    /// branch, and both must collapse a CIMD failure into the same
    /// deliberately generic message -- the detailed `CimdError` is for logs
    /// only.
    #[tokio::test]
    async fn both_resolvers_agree_an_unreachable_cimd_client_does_not_resolve() {
        let state = test_state().await;

        let token_side = resolve_client(&state, CIMD_CLIENT_ID)
            .await
            .expect_err("unreachable CIMD client_id must fail /token");
        let authorize_side = resolve_client_redirect_uris(&state, CIMD_CLIENT_ID, "state-id")
            .await
            .expect_err("unreachable CIMD client_id must fail /authorize");

        for error in [token_side, authorize_side] {
            match error {
                AuthError::Validation(message) => assert_eq!(
                    message,
                    "client_id metadata document is invalid or unreachable",
                ),
                other => panic!("expected Validation, got {other:?}"),
            }
        }
    }
}
