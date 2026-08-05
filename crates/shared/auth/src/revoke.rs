//! RFC 7009 OAuth 2.0 Token Revocation (`POST /revoke`).
//!
//! Two properties drive every decision in this module:
//!
//! 1. **Revocation is idempotent and never an oracle.** RFC 7009 section 2.2
//!    requires HTTP 200 both when a token was revoked and when the client
//!    "submitted an invalid token" -- unknown, already revoked, expired, or
//!    issued to somebody else. A caller must not be able to tell those apart
//!    from the response, so the success path below deliberately discards the
//!    store's did-it-delete boolean.
//! 2. **A client may only revoke its own tokens.** RFC 7009 section 2.1 makes
//!    the server "verify whether the token was issued to the client making the
//!    revocation request". That check is the `client_id` predicate inside
//!    [`crate::sqlite::SqliteStore::revoke_refresh_token`]'s `DELETE`, so a
//!    request naming somebody else's token deletes nothing -- and, per (1),
//!    still answers 200.
//!
//! Only refresh tokens are revocable. Access tokens are self-contained
//! EdDSA-signed JWTs (see `crate::jwt`) validated purely by signature and
//! expiry, with no server-side record to delete and no denylist to add to, so
//! `token_type_hint=access_token` is answered with RFC 7009 section 2.2.1's
//! `unsupported_token_type` rather than a 200 that would misrepresent what
//! happened. Revoking the refresh token still severs renewal; outstanding
//! access tokens age out on their own (one hour by default).

use axum::extract::{ConnectInfo, Form, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use std::net::SocketAddr;
use tracing::{info, warn};

use crate::error::AuthError;
use crate::state::AuthState;
use crate::token_client_auth;
use crate::types::{RevocationRequest, TokenRequest};
use crate::util::{apply_no_store, fingerprint, oauth_error_response, remote_ip};

/// RFC 7009 section 2.1 `token_type_hint` value naming the one token type this
/// server cannot revoke.
const ACCESS_TOKEN_HINT: &str = "access_token";

/// `POST /revoke` -- RFC 7009 token revocation.
///
/// Never logs, echoes, or returns the submitted token value; diagnostics use
/// [`fingerprint`] only.
pub async fn revoke(
    State(state): State<AuthState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(mut request): Form<RevocationRequest>,
) -> Response {
    // Rate-limit first, before client resolution touches the store or (for a
    // CIMD-shaped `client_id`) makes an outbound metadata fetch -- the same
    // reasoning that guards `/token`, which shares this limiter.
    if let Err(error) = state.check_token_rate_limit(remote_ip(addr)).await {
        return RevocationEndpointError::Auth(error).into_response();
    }
    match revoke_token(&state, &headers, &mut request).await {
        Ok(()) => revocation_success(),
        Err(error) => error.into_response(),
    }
}

async fn revoke_token(
    state: &AuthState,
    headers: &HeaderMap,
    request: &mut RevocationRequest,
) -> Result<(), RevocationEndpointError> {
    normalize_credentials(headers, request)?;
    // Required even for public clients: it is the only thing scoping the
    // delete to the caller's own tokens, so without it there is no way to
    // honour RFC 7009 section 2.1's ownership check. Rejecting here is not an
    // oracle -- the outcome depends solely on the request's own shape.
    let client_id = request
        .client_id
        .as_deref()
        .ok_or_else(|| AuthError::Validation("missing `client_id` parameter".to_string()))?;
    token_client_auth::authenticate_oauth_client(
        state,
        client_id,
        request.client_secret.as_deref(),
        request.client_assertion_type.as_deref(),
        request.client_assertion.as_deref(),
    )
    .await?;

    // Checked after client authentication so an unauthenticated caller learns
    // nothing about which token types this server supports. Decided purely
    // from the hint, never from a lookup, so it cannot leak token existence.
    if request.token_type_hint.as_deref() == Some(ACCESS_TOKEN_HINT) {
        warn!(
            client_id = %client_id,
            "oauth revocation rejected: access tokens are stateless and cannot be revoked"
        );
        return Err(RevocationEndpointError::UnsupportedTokenType);
    }

    // Any other hint -- including a bogus one -- is ignored, per RFC 7009
    // section 2.2: "An invalid token type hint value is ignored by the
    // authorization server and does not influence the revocation response."
    let revoked = state
        .store
        .revoke_refresh_token(&request.token, client_id)
        .await?;
    // `revoked` reaches the operator's logs and stops there. Branching the
    // HTTP response on it is exactly the token-existence oracle RFC 7009
    // forbids.
    info!(
        client_id = %client_id,
        token_id = %fingerprint(&request.token),
        revoked,
        "oauth revocation processed"
    );
    Ok(())
}

/// Fold RFC 6749 section 2.3.1 HTTP Basic credentials into the body parameters
/// so `/revoke` accepts exactly the client-authentication shapes `/token`
/// does, without reimplementing security-sensitive credential parsing here.
///
/// [`token_client_auth::normalize_client_credentials`] is written against
/// [`TokenRequest`] because that was the only request shape it had to serve,
/// but every field it touches (`client_id`, `client_secret`,
/// `client_assertion`, `client_assertion_type`, `assertion`) is client
/// authentication rather than grant data, and is common to both requests. The
/// shim's `grant_type` is left empty deliberately: the sole branch that reads
/// it folds a JWT-bearer *authorization grant*, which a revocation request
/// never carries.
fn normalize_credentials(
    headers: &HeaderMap,
    request: &mut RevocationRequest,
) -> Result<(), AuthError> {
    // Only the four credential fields are meaningful here; `..Default::default()`
    // keeps them the only thing a reader has to check. `grant_type` stays empty,
    // which is safe: the one branch that reads it (`adopt_jwt_bearer_assertion`)
    // can never fire, because `RevocationRequest` carries no `assertion` field.
    let mut shim = TokenRequest {
        client_id: request.client_id.take(),
        client_secret: request.client_secret.take(),
        client_assertion_type: request.client_assertion_type.take(),
        client_assertion: request.client_assertion.take(),
        ..Default::default()
    };
    token_client_auth::normalize_client_credentials(headers, &mut shim)?;
    request.client_id = shim.client_id;
    request.client_secret = shim.client_secret;
    request.client_assertion_type = shim.client_assertion_type;
    request.client_assertion = shim.client_assertion;
    Ok(())
}

/// RFC 7009 section 2.2: 200 with no body. "The content of the response body
/// is ignored by the client as all necessary information is conveyed in the
/// response code."
fn revocation_success() -> Response {
    apply_no_store(StatusCode::OK.into_response())
}

/// Failures that reach the client as an RFC 6749 section 5.2 error object,
/// which RFC 7009 section 2.2.1 adopts wholesale for this endpoint.
enum RevocationEndpointError {
    Auth(AuthError),
    UnsupportedTokenType,
}

impl From<AuthError> for RevocationEndpointError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl RevocationEndpointError {
    fn oauth_error(&self) -> &'static str {
        match self {
            Self::UnsupportedTokenType => "unsupported_token_type",
            Self::Auth(AuthError::InvalidGrant(_)) => "invalid_grant",
            Self::Auth(AuthError::InvalidScope(_)) => "invalid_scope",
            Self::Auth(AuthError::AuthFailed(_) | AuthError::InvalidAccessToken) => {
                "invalid_client"
            }
            Self::Auth(AuthError::RateLimited { .. }) => "temporarily_unavailable",
            Self::Auth(AuthError::Validation(_)) => "invalid_request",
            Self::Auth(
                AuthError::Config(_)
                | AuthError::Storage(_)
                | AuthError::Network(_)
                | AuthError::Server(_)
                | AuthError::Decode(_)
                | AuthError::InsecurePermissions { .. },
            ) => "server_error",
        }
    }

    fn log_kind(&self) -> &'static str {
        match self {
            Self::Auth(error) => error.kind(),
            Self::UnsupportedTokenType => "unsupported_token_type",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::UnsupportedTokenType
            | Self::Auth(
                AuthError::InvalidGrant(_) | AuthError::InvalidScope(_) | AuthError::Validation(_),
            ) => StatusCode::BAD_REQUEST,
            Self::Auth(AuthError::AuthFailed(_) | AuthError::InvalidAccessToken) => {
                StatusCode::UNAUTHORIZED
            }
            Self::Auth(AuthError::RateLimited { .. }) => StatusCode::TOO_MANY_REQUESTS,
            // A storage fault must never look like a successful revocation:
            // RFC 7009 section 2.2.1 tells the client to assume the token
            // still exists when the server reports a failure, which is
            // exactly right here.
            Self::Auth(
                AuthError::Config(_)
                | AuthError::Storage(_)
                | AuthError::Network(_)
                | AuthError::Server(_)
                | AuthError::Decode(_)
                | AuthError::InsecurePermissions { .. },
            ) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn description(&self) -> String {
        match self {
            Self::Auth(error) => error.public_message().to_string(),
            Self::UnsupportedTokenType => {
                "access tokens are stateless and cannot be revoked; revoke the refresh token \
                 instead"
                    .to_string()
            }
        }
    }

    fn retry_after_ms(&self) -> Option<u64> {
        match self {
            Self::Auth(AuthError::RateLimited { retry_after_ms, .. }) => Some(*retry_after_ms),
            _ => None,
        }
    }
}

impl IntoResponse for RevocationEndpointError {
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

#[cfg(test)]
#[path = "revoke_tests.rs"]
mod tests;
