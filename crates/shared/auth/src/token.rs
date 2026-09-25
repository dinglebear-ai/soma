use axum::extract::{ConnectInfo, Form, State};
use axum::{
    Json,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use subtle::ConstantTimeEq;
use tracing::{info, warn};

use crate::error::AuthError;
use crate::jwt::AccessClaims;
use crate::state::AuthState;
use crate::token_client_auth;
use crate::types::AuthorizationCodeRow;
use crate::types::{RefreshTokenRow, TokenRequest, TokenResponse};
use crate::util::{
    apply_no_store, duration_secs_usize, expires_at, fingerprint, now_unix, oauth_error_response,
    random_token, remote_ip, timestamp_usize,
};

pub async fn token(
    State(state): State<AuthState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(mut request): Form<TokenRequest>,
) -> Response {
    // Rate-limit before any parsing or client resolution. Client
    // authentication now runs ahead of grant work, and for a CIMD-shaped
    // `client_id` that means an outbound metadata fetch - reachable without a
    // valid authorization code. Unlimited, that turns `/token` into both a
    // self-DoS (queued 5s-timeout fetches) and an unauthenticated trigger for
    // outbound requests to attacker-chosen hosts. `/authorize` and `/register`
    // have always guarded this way; `/token` never called its own limiter.
    if let Err(error) = state.check_token_rate_limit(remote_ip(addr)).await {
        return TokenEndpointError::Auth(error).into_response();
    }
    if let Err(error) = token_client_auth::normalize_client_credentials(&headers, &mut request) {
        return TokenEndpointError::Auth(error).into_response();
    }
    info!(
        grant_type = %request.grant_type,
        client_id = request.client_id.as_deref().unwrap_or("<missing>"),
        requested_resource = request.resource.as_deref().unwrap_or("<default>"),
        "oauth token request received"
    );

    match dispatch_grant(state, request).await {
        Ok(response) => response.into_response(),
        Err(error) => error.into_response(),
    }
}

async fn dispatch_grant(
    state: AuthState,
    request: TokenRequest,
) -> Result<TokenResponseWithCache, TokenEndpointError> {
    let response = match request.grant_type.as_str() {
        "authorization_code" => {
            authenticate_client(&state, &request).await?;
            authorization_code_grant(state, request).await
        }
        "refresh_token" => {
            authenticate_client(&state, &request).await?;
            refresh_token_grant(state, request).await
        }
        // Authenticated inside `machine_grant`, before any token material is
        // minted - so every arm of this table authenticates first, even though
        // only the two delegation arms say so explicitly.
        "client_credentials" | token_client_auth::JWT_BEARER_GRANT_TYPE => {
            machine_client_grant(state, request).await
        }
        other => {
            warn!(grant_type = %other, "oauth token rejected: unsupported grant type");
            return Err(TokenEndpointError::UnsupportedGrantType(other.to_string()));
        }
    };
    response
        .map(|response| TokenResponseWithCache(Json(response)))
        .map_err(TokenEndpointError::Auth)
}

/// RFC 6749 section 3.2.1 client authentication for the user-delegation
/// grants, run *before* any grant-specific work so an unauthenticated request
/// can never consume a single-use authorization code.
///
/// Public clients (`token_endpoint_auth_method = "none"`, which is what
/// dynamic registration and CIMD produce by default) present no secret and no
/// assertion and are accepted exactly as they were before this check existed.
/// Confidential clients must satisfy the method they registered.
async fn authenticate_client(state: &AuthState, request: &TokenRequest) -> Result<(), AuthError> {
    let client_id = request
        .client_id
        .as_deref()
        .ok_or_else(|| AuthError::Validation("missing `client_id` parameter".to_string()))?;
    if recorded_public_client(state, request, client_id).await? {
        return authenticate_recorded_public_client(request);
    }
    token_client_auth::authenticate_oauth_client(
        state,
        client_id,
        request.client_secret.as_deref(),
        request.client_assertion_type.as_deref(),
        request.client_assertion.as_deref(),
    )
    .await
}

/// Whether the grant being redeemed was issued to a client that registered
/// `token_endpoint_auth_method = "none"`, as recorded on the grant itself.
///
/// This is the whole point of storing the method at issuance. Resolving the
/// client instead means, for a CIMD-shaped (`https://...`) `client_id`, a live
/// metadata fetch — so a valid, unrevoked refresh token failed with
/// `invalid_client` for as long as the client's own metadata host was
/// unreachable (up to the 60s negative-cache window, repeating until it came
/// back). A public client presents no credentials to check against that
/// document, so the fetch buys nothing for it.
///
/// Only the public case is short-circuited. `private_key_jwt` still resolves:
/// its JWKS must be read fresh because the client may have rotated keys, and
/// such a client is presenting an assertion anyway, so the fetch is inherent.
/// A `None` record (legacy row, or a client that could not be resolved at
/// issuance) also resolves — unknown never means public.
///
/// Machine clients are excluded: their credentials come from server config,
/// not from a registration record, and `authenticate_oauth_client` checks them
/// first. Skipping it for a config-declared client id would drop that check.
async fn recorded_public_client(
    state: &AuthState,
    request: &TokenRequest,
    client_id: &str,
) -> Result<bool, AuthError> {
    if state
        .config
        .machine_clients
        .iter()
        .any(|client| client.client_id == client_id)
    {
        return Ok(false);
    }
    let recorded = match request.grant_type.as_str() {
        "authorization_code" => match request.code.as_deref() {
            Some(code) => state.store.auth_code_client_auth_method(code).await?,
            None => None,
        },
        "refresh_token" => match request.refresh_token.as_deref() {
            Some(token) => state.store.refresh_token_client_auth_method(token).await?,
            None => None,
        },
        _ => None,
    };
    Ok(recorded.as_deref() == Some("none"))
}

/// The public-client half of `authenticate_oauth_client`, decided locally.
///
/// Byte-for-byte the same verdict that arm reaches — a client registered with
/// `token_endpoint_auth_method = "none"` that presents a `client_secret` or a
/// `client_assertion` is rejected — just without the client resolution that
/// produced the method, which the grant already told us.
fn authenticate_recorded_public_client(request: &TokenRequest) -> Result<(), AuthError> {
    if request.client_secret.is_some() || request.client_assertion.is_some() {
        warn!(
            grant_type = %request.grant_type,
            "oauth token rejected: public client presented client credentials"
        );
        return Err(token_client_auth::invalid_client());
    }
    Ok(())
}

/// `client_credentials` / JWT-bearer machine grant. The client acts for
/// itself, so the token's subject is the client id and no refresh token is
/// issued (RFC 6749 section 4.4.3): the client can always re-authenticate.
async fn machine_client_grant(
    state: AuthState,
    request: TokenRequest,
) -> Result<TokenResponse, AuthError> {
    let grant = token_client_auth::machine_grant(&state, &request).await?;
    info!(
        grant_type = %request.grant_type,
        client_id = %grant.client_id,
        resource = %grant.resource,
        scope = %grant.scope,
        "oauth machine grant authenticated client"
    );
    build_token_response(
        &state,
        grant.client_id,
        grant.subject,
        grant.resource,
        grant.scope,
        None,
    )
}

enum TokenEndpointError {
    Auth(AuthError),
    UnsupportedGrantType(String),
}

impl From<AuthError> for TokenEndpointError {
    fn from(error: AuthError) -> Self {
        Self::Auth(error)
    }
}

impl TokenEndpointError {
    fn oauth_error(&self) -> &'static str {
        match self {
            Self::Auth(AuthError::InvalidGrant(_)) => "invalid_grant",
            Self::Auth(AuthError::InvalidScope(_)) => "invalid_scope",
            Self::UnsupportedGrantType(_) => "unsupported_grant_type",
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
            Self::UnsupportedGrantType(_) => "unsupported_grant_type",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::Auth(
                AuthError::InvalidGrant(_) | AuthError::InvalidScope(_) | AuthError::Validation(_),
            )
            | Self::UnsupportedGrantType(_) => StatusCode::BAD_REQUEST,
            Self::Auth(AuthError::AuthFailed(_) | AuthError::InvalidAccessToken) => {
                StatusCode::UNAUTHORIZED
            }
            Self::Auth(AuthError::RateLimited { .. }) => StatusCode::TOO_MANY_REQUESTS,
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
            Self::Auth(error) => error.to_string(),
            Self::UnsupportedGrantType(grant_type) => {
                format!("unsupported grant_type `{grant_type}`")
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

impl IntoResponse for TokenEndpointError {
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

struct TokenResponseWithCache(Json<TokenResponse>);

impl IntoResponse for TokenResponseWithCache {
    fn into_response(self) -> Response {
        apply_no_store(self.0.into_response())
    }
}

async fn authorization_code_grant(
    state: AuthState,
    request: TokenRequest,
) -> Result<TokenResponse, AuthError> {
    let requested_resource = request
        .resource
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_end_matches('/').to_string());
    crate::authorize::validate_resource(&state, request.resource.as_deref())?;
    let code = require_field(request.code, "code")?;
    let client_id = require_field(request.client_id, "client_id")?;
    let redirect_uri = require_field(request.redirect_uri, "redirect_uri")?;
    let code_verifier = require_field(request.code_verifier, "code_verifier")?;
    let auth_code_id = fingerprint(&code);
    info!(
        grant_type = "authorization_code",
        client_id = %client_id,
        auth_code_id = %auth_code_id,
        redirect_uri = %redirect_uri,
        requested_resource = requested_resource.as_deref().unwrap_or("<authorization-code-resource>"),
        "oauth authorization_code grant redeeming local code"
    );

    let row = state.store.redeem_auth_code(&code).await.map_err(|error| {
        warn!(
            auth_code_id = %auth_code_id,
            client_id = %client_id,
            error = %error,
            "oauth token rejected: authorization code is invalid, expired, or already redeemed"
        );
        error
    })?;
    validate_authorization_code_row(
        &row,
        &client_id,
        &redirect_uri,
        &code_verifier,
        &auth_code_id,
    )?;
    if let Some(requested_resource) = requested_resource
        && requested_resource != row.resource
    {
        warn!(
            auth_code_id = %auth_code_id,
            requested_resource = %requested_resource,
            stored_resource = %row.resource,
            "oauth token rejected: resource does not match authorization code"
        );
        return Err(AuthError::InvalidGrant(
            "resource does not match the authorization code".to_string(),
        ));
    }

    let refresh_token = if let Some(provider_refresh_token) = row.provider_refresh_token {
        let refresh_token = random_token(24)?;
        let created_at = now_unix();
        state
            .store
            .upsert_refresh_token(RefreshTokenRow {
                refresh_token: refresh_token.clone(),
                client_id: row.client_id.clone(),
                subject: row.subject.clone(),
                resource: row.resource.clone(),
                scope: row.scope.clone(),
                provider: row.provider.clone(),
                provider_refresh_token: Some(provider_refresh_token),
                created_at,
                expires_at: expires_at(
                    created_at,
                    state.config.refresh_token_ttl,
                    &format!("{}_AUTH_REFRESH_TOKEN_TTL_SECS", state.config.env_prefix),
                )?,
                // The refresh token inherits the authorization code's contract
                // so later refreshes authenticate the same way this exchange
                // did, with no client resolution in between.
                token_endpoint_auth_method: row.token_endpoint_auth_method.clone(),
            })
            .await?;
        info!(
            grant_type = "authorization_code",
            client_id = %row.client_id,
            auth_code_id = %auth_code_id,
            subject_id = %fingerprint(&row.subject),
            resource = %row.resource,
            scope = %row.scope,
            "oauth authorization_code grant issued lab access token and refresh token"
        );
        Some(refresh_token)
    } else {
        info!(
            grant_type = "authorization_code",
            client_id = %row.client_id,
            auth_code_id = %auth_code_id,
            subject_id = %fingerprint(&row.subject),
            resource = %row.resource,
            scope = %row.scope,
            "oauth authorization_code grant issued lab access token without refresh token"
        );
        None
    };

    let resource = if row.resource.trim().is_empty() {
        crate::metadata::canonical_resource_url(&state)
    } else {
        row.resource
    };
    build_token_response(
        &state,
        row.client_id,
        row.subject,
        resource,
        row.scope,
        refresh_token,
    )
}

async fn refresh_token_grant(
    state: AuthState,
    request: TokenRequest,
) -> Result<TokenResponse, AuthError> {
    let requested_resource = request
        .resource
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| crate::authorize::validate_resource(&state, Some(value)))
        .transpose()?;
    let client_id = require_field(request.client_id, "client_id")?;
    let refresh_token = require_field(request.refresh_token, "refresh_token")?;
    let refresh_token_id = fingerprint(&refresh_token);
    info!(
        grant_type = "refresh_token",
        client_id = %client_id,
        refresh_token_id = %refresh_token_id,
        requested_resource = requested_resource.as_deref().unwrap_or("<refresh-token-resource>"),
        "oauth refresh_token grant received"
    );
    let stored = state
        .store
        .find_refresh_token(&refresh_token)
        .await?
        .ok_or_else(|| {
            warn!(
                refresh_token_id = %refresh_token_id,
                client_id = %client_id,
                "oauth token rejected: unknown or expired refresh token"
            );
            AuthError::InvalidGrant("unknown refresh_token".to_string())
        })?;
    if stored.client_id != client_id {
        warn!(
            refresh_token_id = %refresh_token_id,
            requested_client_id = %client_id,
            stored_client_id = %stored.client_id,
            "oauth token rejected: client_id does not match refresh token"
        );
        return Err(AuthError::InvalidGrant(
            "client_id does not match the refresh token".to_string(),
        ));
    }
    let stored_resource = if stored.resource.trim().is_empty() {
        crate::metadata::canonical_resource_url(&state)
    } else {
        stored.resource.clone()
    };
    if let Some(requested_resource) = requested_resource
        && requested_resource != stored_resource
    {
        warn!(
            refresh_token_id = %refresh_token_id,
            requested_resource = %requested_resource,
            stored_resource = %stored_resource,
            "oauth token rejected: resource does not match refresh token"
        );
        return Err(AuthError::InvalidGrant(
            "resource does not match the refresh token".to_string(),
        ));
    }

    let Some(provider_refresh_token) = stored.provider_refresh_token.clone() else {
        warn!(
            refresh_token_id = %refresh_token_id,
            client_id = %stored.client_id,
            "oauth token rejected: refresh token is not backed by an upstream refresh token"
        );
        return Err(AuthError::InvalidGrant(
            "refresh token is not backed by an upstream refresh token".to_string(),
        ));
    };

    // Refresh upstream before consuming the local token. If the provider or
    // id-token verification fails, the client can retry the same local
    // refresh token instead of being stranded with an unreturned replacement.
    let provider = state.provider(&stored.provider)?;
    // Defense-in-depth: GitHubProvider::exchange_code never sets
    // provider_refresh_token, so a `refresh_tokens` row naming
    // `provider = "github"` with a non-null `provider_refresh_token` should
    // be unreachable through normal flows — but the DB layer doesn't enforce
    // that invariant, and a hand-inserted or corrupted row would otherwise
    // silently reach `GitHubProvider::refresh`'s unconditional error. Fail
    // loudly and clearly here instead, at the actual choke point, in both
    // debug and release builds.
    if provider.provider_id() == "github" {
        return Err(AuthError::Server(
            "refresh token names provider `github`, which never issues upstream refresh \
             tokens and does not support token refresh — this refresh token row should be \
             unreachable; the underlying GitHub OAuth App requires the user to \
             re-authenticate once their local soma-issued refresh token expires"
                .to_string(),
        ));
    }
    let exchange = provider.refresh(&provider_refresh_token).await?;

    // A refresh is a new authorization decision, not merely token rotation.
    // Re-check the provider's freshly verified identity against the current
    // allowlist so removing an operator actually stops renewable access.
    let allowed = state.resolve_allowed_emails().await?;
    crate::authorize::check_email_allowlist(
        provider.provider_id(),
        exchange.email.as_deref(),
        exchange.email_verified,
        &allowed,
    )?;

    let refreshed_expires_at = expires_at(
        now_unix(),
        state.config.refresh_token_ttl,
        &format!("{}_AUTH_REFRESH_TOKEN_TTL_SECS", state.config.env_prefix),
    )?;
    let subject =
        crate::oauth_provider::namespaced_subject(provider.provider_id(), &exchange.subject);
    if subject != stored.subject {
        warn!(
            refresh_token_id = %refresh_token_id,
            stored_subject_id = %fingerprint(&stored.subject),
            refreshed_subject_id = %fingerprint(&subject),
            provider = provider.provider_id(),
            "oauth token rejected: refreshed provider identity does not match refresh token"
        );
        return Err(AuthError::InvalidGrant(
            "refreshed provider identity does not match the refresh token".to_string(),
        ));
    }
    let next_provider_refresh_token = exchange
        .refresh_token
        .clone()
        .unwrap_or_else(|| provider_refresh_token.clone());
    // The current allowlist check above is the admin gate. Re-apply admin
    // elevation for older refresh-token rows that predate scope elevation.
    let elevated_scope = crate::authorize::elevate_scope_for_allowed_user(
        &stored.scope,
        &state.config.default_scope,
    );

    state
        .store
        .upsert_refresh_token(RefreshTokenRow {
            refresh_token: refresh_token.clone(),
            client_id: stored.client_id.clone(),
            subject: subject.clone(),
            resource: stored_resource.clone(),
            scope: elevated_scope.clone(),
            provider: stored.provider.clone(),
            provider_refresh_token: Some(next_provider_refresh_token),
            created_at: stored.created_at,
            expires_at: refreshed_expires_at,
            // Preserved verbatim across the rewrite: the grant's contract does
            // not change just because it was refreshed.
            token_endpoint_auth_method: stored.token_endpoint_auth_method.clone(),
        })
        .await?;

    info!(
        grant_type = "refresh_token",
        client_id = %stored.client_id,
        refresh_token_id = %refresh_token_id,
        subject_id = %fingerprint(&subject),
        provider = provider.provider_id(),
        resource = %stored_resource,
        scope = %elevated_scope,
        "oauth refresh_token grant refreshed stable local token and issued new access token"
    );

    build_token_response(
        &state,
        stored.client_id,
        subject,
        stored_resource,
        elevated_scope,
        Some(refresh_token),
    )
}

fn build_token_response(
    state: &AuthState,
    client_id: String,
    subject: String,
    resource: String,
    scope: String,
    refresh_token: Option<String>,
) -> Result<TokenResponse, AuthError> {
    let issuer = crate::metadata::public_base_url(state);
    let now = timestamp_usize(now_unix(), "current unix timestamp")?;
    let access_token_ttl = duration_secs_usize(
        state.config.access_token_ttl,
        &format!("{}_AUTH_ACCESS_TOKEN_TTL_SECS", state.config.env_prefix),
    )?;
    let subject_id = fingerprint(&subject);
    let access_token = state.signing_keys.issue_access_token(&AccessClaims {
        iss: issuer,
        sub: subject.clone(),
        aud: resource.clone(),
        exp: now.checked_add(access_token_ttl).ok_or_else(|| {
            AuthError::Config(format!(
                "{}_AUTH_ACCESS_TOKEN_TTL_SECS exceeds supported range",
                state.config.env_prefix
            ))
        })?,
        iat: now,
        jti: random_token(18)?,
        scope: scope.clone(),
        azp: client_id.clone(),
    })?;
    info!(
        client_id = %client_id,
        subject_id = %subject_id,
        resource = %resource,
        scope = %scope,
        expires_in_secs = state.config.access_token_ttl.as_secs(),
        refresh_token_issued = refresh_token.is_some(),
        "oauth token response minted access token"
    );
    Ok(TokenResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in: state.config.access_token_ttl.as_secs(),
        refresh_token,
        scope,
    })
}

fn require_field(value: Option<String>, field: &str) -> Result<String, AuthError> {
    value.ok_or_else(|| AuthError::Validation(format!("missing `{field}` parameter")))
}

fn pkce_challenge(code_verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()))
}

fn validate_authorization_code_row(
    row: &AuthorizationCodeRow,
    client_id: &str,
    redirect_uri: &str,
    code_verifier: &str,
    auth_code_id: &str,
) -> Result<(), AuthError> {
    if row.client_id != client_id {
        warn!(
            auth_code_id = %auth_code_id,
            requested_client_id = %client_id,
            stored_client_id = %row.client_id,
            "oauth token rejected: client_id does not match authorization code"
        );
        return Err(AuthError::InvalidGrant(
            "client_id does not match the authorization code".to_string(),
        ));
    }
    if row.redirect_uri != redirect_uri {
        warn!(
            auth_code_id = %auth_code_id,
            requested_redirect_uri = %redirect_uri,
            stored_redirect_uri = %row.redirect_uri,
            "oauth token rejected: redirect_uri does not match authorization code"
        );
        return Err(AuthError::InvalidGrant(
            "redirect_uri does not match the authorization code".to_string(),
        ));
    }
    if row.code_challenge_method != "S256" {
        warn!(
            auth_code_id = %auth_code_id,
            code_challenge_method = %row.code_challenge_method,
            "oauth token rejected: unsupported PKCE method on authorization code"
        );
        return Err(AuthError::InvalidGrant(
            "authorization code uses an unsupported PKCE method".to_string(),
        ));
    }
    if !bool::from(
        pkce_challenge(code_verifier)
            .as_bytes()
            .ct_eq(row.code_challenge.as_bytes()),
    ) {
        warn!(
            auth_code_id = %auth_code_id,
            client_id = %row.client_id,
            "oauth token rejected: code_verifier did not match authorization code"
        );
        return Err(AuthError::InvalidGrant(
            "code_verifier does not match the authorization code".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use base64::Engine as _;
    use ed25519_dalek::pkcs8::EncodePrivateKey as _;
    use jsonwebtoken::dangerous::insecure_decode;
    use tower::util::ServiceExt;
    use url::Url;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use axum::Router;
    use axum::extract::connect_info::MockConnectInfo;
    use std::net::SocketAddr;

    use crate::config::MachineClientConfig;
    use crate::google::GoogleProvider;

    // `oneshot` bypasses the live `into_make_service_with_connect_info` layer,
    // so `/token`'s rate-limit `ConnectInfo<SocketAddr>` extractor would be
    // missing and every request would 500. Wrap the real router with a mock
    // peer address, matching the helper in `authorize.rs`.
    fn router(state: AuthState) -> Router {
        crate::routes::router(state)
            .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 9002))))
    }
    use crate::state::AuthState;

    use super::super::authorize::tests::{
        test_auth_config, test_auth_state_with_config, test_auth_state_with_mock_google,
        test_auth_state_with_registered_client,
    };

    async fn test_auth_state_with_failing_google_refresh() -> AuthState {
        let state = test_auth_state_with_registered_client().await;
        let server = Box::leak(Box::new(MockServer::start().await));
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "error": "temporarily_unavailable"
            })))
            .mount(server)
            .await;
        let google = GoogleProvider::new(
            "client-id".to_string(),
            "client-secret".to_string(),
            Url::parse("https://lab.example.com/auth/google/callback").unwrap(),
        )
        .unwrap()
        .with_endpoints(
            server.uri().parse::<Url>().unwrap(),
            server.uri().parse::<Url>().unwrap().join("/token").unwrap(),
        );
        AuthState::for_tests(
            (*state.config).clone(),
            state.store.clone(),
            (*state.signing_keys).clone(),
            AuthState::google_only_providers(google),
        )
    }

    #[tokio::test]
    async fn token_endpoint_mints_lab_jwt_and_refresh_token() {
        let state = test_auth_state_with_registered_client().await;
        seed_authorization_code(&state).await;
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(
                        header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .body(Body::from("grant_type=authorization_code&code=lab-code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&code_verifier=verifier"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("no-store")
        );
        assert_eq!(
            response
                .headers()
                .get(header::PRAGMA)
                .and_then(|value| value.to_str().ok()),
            Some("no-cache")
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["access_token"].is_string());
        assert!(json["refresh_token"].is_string());
        let access_token = json["access_token"].as_str().expect("access token string");
        let claims = insecure_decode::<crate::jwt::AccessClaims>(access_token)
            .expect("decode access token")
            .claims;
        assert_eq!(claims.aud, "https://lab.example.com/mcp");
    }

    #[tokio::test]
    async fn token_endpoint_omits_refresh_token_without_upstream_refresh_capability() {
        let state = test_auth_state_with_registered_client().await;
        seed_authorization_code_without_provider_refresh(&state).await;
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(
                        header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .body(Body::from("grant_type=authorization_code&code=lab-code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&code_verifier=verifier"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("no-store")
        );
        assert_eq!(
            response
                .headers()
                .get(header::PRAGMA)
                .and_then(|value| value.to_str().ok()),
            Some("no-cache")
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["access_token"].is_string());
        assert!(json.get("refresh_token").is_none());
    }

    #[tokio::test]
    async fn token_endpoint_redeems_authorization_code_once() {
        let state = test_auth_state_with_registered_client().await;
        seed_authorization_code(&state).await;
        let app = router(state);
        let (a, b) = tokio::join!(
            app.clone().oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(
                        header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .body(Body::from("grant_type=authorization_code&code=lab-code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&code_verifier=verifier"))
                    .unwrap()
            ),
            app.oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(
                        header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .body(Body::from("grant_type=authorization_code&code=lab-code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&code_verifier=verifier"))
                    .unwrap()
            )
        );
        let a = a.unwrap();
        let b = b.unwrap();
        assert!(a.status() == StatusCode::OK || b.status() == StatusCode::OK);
        assert!(a.status() == StatusCode::BAD_REQUEST || b.status() == StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn token_endpoint_rejects_expired_authorization_code() {
        let state = test_auth_state_with_registered_client().await;
        seed_authorization_code_with_expiry(&state, crate::util::now_unix() - 1).await;
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(
                        header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .body(Body::from("grant_type=authorization_code&code=lab-code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&code_verifier=verifier"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("no-store")
        );
        assert_eq!(
            response
                .headers()
                .get(header::PRAGMA)
                .and_then(|value| value.to_str().ok()),
            Some("no-cache")
        );
    }

    #[tokio::test]
    async fn token_endpoint_errors_use_oauth_error_shape() {
        let state = test_auth_state_with_registered_client().await;
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "grant_type=refresh_token&refresh_token=missing&client_id=client",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("no-store")
        );
        assert_eq!(
            response
                .headers()
                .get(header::PRAGMA)
                .and_then(|value| value.to_str().ok()),
            Some("no-cache")
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "invalid_grant");
        assert_eq!(json["error_description"], "unknown refresh_token");
        assert!(json.get("kind").is_none());
        assert!(json.get("message").is_none());
    }

    #[tokio::test]
    async fn token_endpoint_unsupported_grant_type_uses_oauth_error_shape() {
        let state = test_auth_state_with_registered_client().await;
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("grant_type=password&client_id=client"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "unsupported_grant_type");
        assert_eq!(
            json["error_description"],
            "unsupported grant_type `password`"
        );
    }

    #[tokio::test]
    async fn token_endpoint_refresh_grant_sets_cache_headers() {
        let state = test_auth_state_with_mock_google().await;
        state
            .store
            .upsert_refresh_token(crate::types::RefreshTokenRow {
                refresh_token: "refresh-token".to_string(),
                client_id: "client".to_string(),
                subject: "google-subject-123".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                provider_refresh_token: Some("provider-refresh".to_string()),
                created_at: crate::util::now_unix() - 60,
                expires_at: crate::util::now_unix() + 3600,
                token_endpoint_auth_method: None,
            })
            .await
            .unwrap();
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "grant_type=refresh_token&refresh_token=refresh-token&client_id=client",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("no-store")
        );
        assert_eq!(
            response
                .headers()
                .get(header::PRAGMA)
                .and_then(|value| value.to_str().ok()),
            Some("no-cache")
        );
    }

    #[tokio::test]
    async fn token_endpoint_refresh_grant_preserves_stored_resource_when_omitted() {
        let state = test_auth_state_with_mock_google().await;
        state
            .store
            .upsert_refresh_token(crate::types::RefreshTokenRow {
                refresh_token: "refresh-token".to_string(),
                client_id: "client".to_string(),
                subject: "google-subject-123".to_string(),
                resource: "https://mcp.example.com/syslog".to_string(),
                scope: "mcp:read mcp:write".to_string(),
                provider: "google".to_string(),
                provider_refresh_token: Some("provider-refresh".to_string()),
                created_at: crate::util::now_unix() - 60,
                expires_at: crate::util::now_unix() + 3600,
                token_endpoint_auth_method: None,
            })
            .await
            .unwrap();
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "grant_type=refresh_token&refresh_token=refresh-token&client_id=client",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let access_token = json["access_token"].as_str().expect("access token string");
        let claims = insecure_decode::<crate::jwt::AccessClaims>(access_token)
            .expect("decode access token")
            .claims;
        assert_eq!(claims.aud, "https://mcp.example.com/syslog");
        assert_eq!(claims.scope, "mcp:read mcp:write lab:admin");
    }

    #[tokio::test]
    async fn token_endpoint_rejects_mismatched_resource_parameter() {
        let state = test_auth_state_with_registered_client().await;
        seed_authorization_code(&state).await;
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(
                        header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .body(Body::from("grant_type=authorization_code&code=lab-code&client_id=client&resource=https%3A%2F%2Fother.example.com%2Fmcp&redirect_uri=http://127.0.0.1:7777/callback&code_verifier=verifier"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn token_endpoint_rejects_expired_refresh_token() {
        let state = test_auth_state_with_registered_client().await;
        state
            .store
            .upsert_refresh_token(crate::types::RefreshTokenRow {
                refresh_token: "refresh-token".to_string(),
                client_id: "client".to_string(),
                subject: "google-subject-123".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                provider_refresh_token: Some("provider-refresh".to_string()),
                created_at: crate::util::now_unix() - 3600,
                expires_at: crate::util::now_unix() - 1,
                token_endpoint_auth_method: None,
            })
            .await
            .unwrap();
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "grant_type=refresh_token&refresh_token=refresh-token&client_id=client",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("no-store")
        );
        assert_eq!(
            response
                .headers()
                .get(header::PRAGMA)
                .and_then(|value| value.to_str().ok()),
            Some("no-cache")
        );
    }

    #[tokio::test]
    async fn token_endpoint_rejects_refresh_token_client_mismatch() {
        let state = test_auth_state_with_registered_client().await;
        // `other-client` must be a *registered* public client, otherwise the
        // request is rejected by client authentication (invalid_client) before
        // reaching the refresh-token/client binding check this test covers.
        state
            .store
            .register_client(crate::types::RegisteredClient {
                client_id: "other-client".to_string(),
                redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
                created_at: crate::util::now_unix(),
                token_endpoint_auth_method: "none".to_string(),
                jwks: None,
            })
            .await
            .unwrap();
        state
            .store
            .upsert_refresh_token(crate::types::RefreshTokenRow {
                refresh_token: "refresh-token".to_string(),
                client_id: "client".to_string(),
                subject: "google-subject-123".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                provider_refresh_token: Some("provider-refresh".to_string()),
                created_at: crate::util::now_unix() - 60,
                expires_at: crate::util::now_unix() + 3600,
                token_endpoint_auth_method: None,
            })
            .await
            .unwrap();
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(
                        header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .body(Body::from(
                        "grant_type=refresh_token&refresh_token=refresh-token&client_id=other-client",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|value| value.to_str().ok()),
            Some("no-store")
        );
        assert_eq!(
            response
                .headers()
                .get(header::PRAGMA)
                .and_then(|value| value.to_str().ok()),
            Some("no-cache")
        );
    }

    #[tokio::test]
    async fn token_endpoint_rejects_refresh_token_without_upstream_refresh_capability() {
        let state = test_auth_state_with_registered_client().await;
        state
            .store
            .upsert_refresh_token(crate::types::RefreshTokenRow {
                refresh_token: "refresh-token".to_string(),
                client_id: "client".to_string(),
                subject: "google-subject-123".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                provider_refresh_token: None,
                created_at: crate::util::now_unix() - 60,
                expires_at: crate::util::now_unix() + 3600,
                token_endpoint_auth_method: None,
            })
            .await
            .unwrap();
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "grant_type=refresh_token&refresh_token=refresh-token&client_id=client",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    async fn seed_authorization_code(state: &AuthState) {
        seed_authorization_code_with_expiry(state, 4_102_444_800).await;
    }

    async fn seed_authorization_code_without_provider_refresh(state: &AuthState) {
        state
            .store
            .insert_auth_code(crate::types::AuthorizationCodeRow {
                code: "lab-code".to_string(),
                client_id: "client".to_string(),
                subject: "google-subject-123".to_string(),
                redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                code_challenge: super::pkce_challenge("verifier"),
                code_challenge_method: "S256".to_string(),
                provider_refresh_token: None,
                created_at: 1_700_000_000,
                expires_at: 4_102_444_800,
                token_endpoint_auth_method: None,
            })
            .await
            .unwrap();
    }

    async fn seed_authorization_code_with_expiry(state: &AuthState, expires_at: i64) {
        state
            .store
            .insert_auth_code(crate::types::AuthorizationCodeRow {
                code: "lab-code".to_string(),
                client_id: "client".to_string(),
                subject: "google-subject-123".to_string(),
                redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                code_challenge: super::pkce_challenge("verifier"),
                code_challenge_method: "S256".to_string(),
                provider_refresh_token: Some("provider-refresh".to_string()),
                created_at: 1_700_000_000,
                expires_at,
                token_endpoint_auth_method: None,
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn refresh_grant_preserves_local_token_on_success() {
        let state = test_auth_state_with_mock_google().await;
        state
            .store
            .upsert_refresh_token(crate::types::RefreshTokenRow {
                refresh_token: "original-token".to_string(),
                client_id: "client".to_string(),
                subject: "google-subject-123".to_string(),
                resource: String::new(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                provider_refresh_token: Some("provider-refresh".to_string()),
                created_at: crate::util::now_unix() - 60,
                expires_at: crate::util::now_unix() + 3600,
                token_endpoint_auth_method: None,
            })
            .await
            .unwrap();
        let app = router(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "grant_type=refresh_token&refresh_token=original-token&client_id=client",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let new_token = json["refresh_token"].as_str().expect("refresh_token");
        assert_eq!(
            new_token, "original-token",
            "local token must remain stable"
        );
        assert!(
            state
                .store
                .find_refresh_token("original-token")
                .await
                .unwrap()
                .is_some(),
            "local refresh token must remain usable after successful refresh"
        );
    }

    #[tokio::test]
    async fn refresh_grant_rejects_identity_removed_from_current_allowlist() {
        let base = test_auth_state_with_mock_google().await;
        let mut config = (*base.config).clone();
        config.admin_email = "different-admin@example.com".to_string();
        let state = AuthState::for_tests(
            config,
            base.store.clone(),
            (*base.signing_keys).clone(),
            (*base.providers).clone(),
        );
        state
            .store
            .upsert_refresh_token(crate::types::RefreshTokenRow {
                refresh_token: "removed-user-token".to_string(),
                client_id: "client".to_string(),
                subject: "google-subject-123".to_string(),
                resource: String::new(),
                scope: "lab lab:admin".to_string(),
                provider: "google".to_string(),
                provider_refresh_token: Some("provider-refresh".to_string()),
                created_at: crate::util::now_unix() - 60,
                expires_at: crate::util::now_unix() + 3600,
                token_endpoint_auth_method: None,
            })
            .await
            .unwrap();

        let response = router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "grant_type=refresh_token&refresh_token=removed-user-token&client_id=client",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(
            state
                .store
                .find_refresh_token("removed-user-token")
                .await
                .unwrap()
                .is_some(),
            "a denied refresh must not mutate the stored grant"
        );
    }

    #[tokio::test]
    async fn refresh_grant_rejects_provider_subject_switch() {
        let state = test_auth_state_with_mock_google().await;
        state
            .store
            .upsert_refresh_token(crate::types::RefreshTokenRow {
                refresh_token: "subject-switch-token".to_string(),
                client_id: "client".to_string(),
                subject: "different-google-subject".to_string(),
                resource: String::new(),
                scope: "lab lab:admin".to_string(),
                provider: "google".to_string(),
                provider_refresh_token: Some("provider-refresh".to_string()),
                created_at: crate::util::now_unix() - 60,
                expires_at: crate::util::now_unix() + 3600,
                token_endpoint_auth_method: None,
            })
            .await
            .unwrap();

        let response = router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "grant_type=refresh_token&refresh_token=subject-switch-token&client_id=client",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn refresh_grant_preserves_original_token_when_upstream_refresh_fails() {
        let state = test_auth_state_with_failing_google_refresh().await;
        state
            .store
            .upsert_refresh_token(crate::types::RefreshTokenRow {
                refresh_token: "recoverable-token".to_string(),
                client_id: "client".to_string(),
                subject: "google-subject-123".to_string(),
                resource: String::new(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                provider_refresh_token: Some("provider-refresh".to_string()),
                created_at: crate::util::now_unix() - 60,
                expires_at: crate::util::now_unix() + 3600,
                token_endpoint_auth_method: None,
            })
            .await
            .unwrap();
        let app = router(state.clone());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "grant_type=refresh_token&refresh_token=recoverable-token&client_id=client",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_ne!(response.status(), StatusCode::OK);
        assert!(
            state
                .store
                .find_refresh_token("recoverable-token")
                .await
                .unwrap()
                .is_some(),
            "local refresh token must remain usable after upstream refresh failure"
        );
    }

    #[tokio::test]
    async fn refresh_grant_elevates_stale_scope_to_admin() {
        // Simulate a refresh token that was issued before elevation was wired in,
        // storing only the base scope ("lab") without "lab:admin".  The refresh
        // grant must re-apply elevate_scope_for_allowed_user so the new access
        // token carries "lab:admin".
        let state = test_auth_state_with_mock_google().await;
        state
            .store
            .upsert_refresh_token(crate::types::RefreshTokenRow {
                refresh_token: "stale-token".to_string(),
                client_id: "client".to_string(),
                subject: "google-subject-123".to_string(),
                resource: String::new(),
                scope: "lab".to_string(), // stale — no lab:admin
                provider: "google".to_string(),
                provider_refresh_token: Some("provider-refresh".to_string()),
                created_at: crate::util::now_unix() - 60,
                expires_at: crate::util::now_unix() + 3600,
                token_endpoint_auth_method: None,
            })
            .await
            .unwrap();
        let app = router(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "grant_type=refresh_token&refresh_token=stale-token&client_id=client",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // Decode the access token and verify the scope was elevated.
        let access_token = json["access_token"].as_str().expect("access_token");
        let claims = state
            .signing_keys
            .validate_access_token_with_issuer(
                access_token,
                "https://lab.example.com/mcp",
                "https://lab.example.com",
            )
            .expect("access token must be valid");
        let scopes: Vec<&str> = claims.scope.split_whitespace().collect();
        assert!(
            scopes.contains(&"lab:admin"),
            "elevated access token must contain lab:admin, got: {:?}",
            scopes
        );
    }

    #[tokio::test]
    async fn refresh_grant_allows_reuse_of_stable_local_token() {
        let state = test_auth_state_with_mock_google().await;
        state
            .store
            .upsert_refresh_token(crate::types::RefreshTokenRow {
                refresh_token: "once-only-token".to_string(),
                client_id: "client".to_string(),
                subject: "google-subject-123".to_string(),
                resource: String::new(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                provider_refresh_token: Some("provider-refresh".to_string()),
                created_at: crate::util::now_unix() - 60,
                expires_at: crate::util::now_unix() + 3600,
                token_endpoint_auth_method: None,
            })
            .await
            .unwrap();
        let app = router(state);
        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "grant_type=refresh_token&refresh_token=once-only-token&client_id=client",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        let replay = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "grant_type=refresh_token&refresh_token=once-only-token&client_id=client",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            replay.status(),
            StatusCode::OK,
            "same local refresh token must be reusable across client restarts"
        );
    }

    #[tokio::test]
    async fn authorization_code_grant_never_mints_a_refresh_token_for_a_github_login() {
        let state = test_auth_state_with_registered_client().await;
        state
            .store
            .insert_auth_code(crate::types::AuthorizationCodeRow {
                code: "github-code".to_string(),
                client_id: "client".to_string(),
                subject: "github:9182310".to_string(),
                redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "github".to_string(),
                code_challenge: super::pkce_challenge("verifier"),
                code_challenge_method: "S256".to_string(),
                provider_refresh_token: None,
                created_at: crate::util::now_unix(),
                expires_at: crate::util::now_unix() + 300,
                token_endpoint_auth_method: None,
            })
            .await
            .unwrap();
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "grant_type=authorization_code&code=github-code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&code_verifier=verifier",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json.get("refresh_token").is_none(),
            "github logins must never receive a local refresh token: {json}"
        );
    }

    /// Engineering-review regression test: a deployment upgrades (backfilling
    /// pre-existing rows to `provider='google'`), then an operator removes a
    /// provider from config while an unexpired `refresh_tokens` row still
    /// names it. `state.provider(...)` must fail clearly (`AuthError::Validation`
    /// → `invalid_request` / 400), not panic or silently fall back to a
    /// different provider.
    #[tokio::test]
    async fn refresh_token_grant_fails_clearly_when_its_provider_is_no_longer_configured() {
        let state = test_auth_state_with_registered_client().await;
        state
            .store
            .upsert_refresh_token(crate::types::RefreshTokenRow {
                refresh_token: "orphaned-refresh".to_string(),
                client_id: "client".to_string(),
                subject: "authelia:some-user".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "authelia".to_string(),
                provider_refresh_token: Some("upstream-refresh".to_string()),
                created_at: crate::util::now_unix(),
                expires_at: crate::util::now_unix() + 3600,
                token_endpoint_auth_method: None,
            })
            .await
            .unwrap();
        // `test_auth_state_with_registered_client` only configures Google —
        // "authelia" is intentionally never configured here, simulating an
        // operator who removed it after this token was issued.
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "grant_type=refresh_token&client_id=client&refresh_token=orphaned-refresh",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "invalid_request");
    }

    /// Defense-in-depth regression test: `GitHubProvider::exchange_code`
    /// never sets `provider_refresh_token`, so a `refresh_tokens` row naming
    /// `provider = "github"` with a non-null `provider_refresh_token` is
    /// unreachable through normal flows — but the DB layer doesn't enforce
    /// that invariant. Hand-insert exactly that row and confirm
    /// `refresh_token_grant` fails loudly with a clear server error instead
    /// of reaching `GitHubProvider::refresh`'s unconditional error.
    #[tokio::test]
    async fn refresh_token_grant_rejects_a_hand_inserted_github_row_with_a_provider_refresh_token()
    {
        let mut config = test_auth_config();
        config.github.client_id = "gh-client".to_string();
        config.github.client_secret = "gh-secret".to_string();
        config.github.scopes = vec!["read:user".to_string(), "user:email".to_string()];
        let state = test_auth_state_with_config(config).await;
        state
            .store
            .register_client(crate::types::RegisteredClient {
                client_id: "client".to_string(),
                redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
                created_at: crate::util::now_unix(),
                token_endpoint_auth_method: "none".to_string(),
                jwks: None,
            })
            .await
            .unwrap();
        state
            .store
            .upsert_refresh_token(crate::types::RefreshTokenRow {
                refresh_token: "github-refresh".to_string(),
                client_id: "client".to_string(),
                subject: "github:9182310".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "github".to_string(),
                provider_refresh_token: Some("hand-inserted-upstream-value".to_string()),
                created_at: crate::util::now_unix(),
                expires_at: crate::util::now_unix() + 3600,
                token_endpoint_auth_method: None,
            })
            .await
            .unwrap();
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "grant_type=refresh_token&client_id=client&refresh_token=github-refresh",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "server_error");
    }

    // --- client authentication + machine grants -------------------------
    //
    // These cover `token_client_auth`, which the token endpoint reaches
    // through `prepare_client_credentials`, `authenticate_client`, and
    // `machine_client_grant`.

    /// Deterministic Ed25519 key standing in for a machine client's signing
    /// key. Test-only material: it authenticates nothing outside this module.
    fn client_assertion_signing_key() -> ed25519_dalek::SigningKey {
        ed25519_dalek::SigningKey::from_bytes(&[7u8; 32])
    }

    const CLIENT_ASSERTION_KID: &str = "client-assertion-kid";

    fn client_assertion_jwks() -> serde_json::Value {
        let public_key = client_assertion_signing_key().verifying_key();
        serde_json::json!({
            "keys": [{
                "kty": "OKP",
                "crv": "Ed25519",
                "alg": "EdDSA",
                "use": "sig",
                "kid": CLIENT_ASSERTION_KID,
                "x": base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(public_key.as_bytes()),
            }]
        })
    }

    fn signed_client_assertion(client_id: &str, jti: &str) -> String {
        let now = crate::util::now_unix();
        let claims = serde_json::json!({
            "iss": client_id,
            "sub": client_id,
            "aud": "https://lab.example.com/token",
            "iat": now,
            "exp": now + 120,
            "jti": jti,
        });
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::EdDSA);
        header.kid = Some(CLIENT_ASSERTION_KID.to_string());
        let der = client_assertion_signing_key().to_pkcs8_der().unwrap();
        jsonwebtoken::encode(
            &header,
            &claims,
            &jsonwebtoken::EncodingKey::from_ed_der(der.as_bytes()),
        )
        .unwrap()
    }

    fn secret_machine_client() -> MachineClientConfig {
        MachineClientConfig {
            client_id: "machine".to_string(),
            client_secret: Some("machine-secret".to_string()),
            jwks: None,
            scopes: vec!["lab".to_string()],
            resources: vec!["https://lab.example.com/mcp".to_string()],
        }
    }

    fn assertion_machine_client() -> MachineClientConfig {
        MachineClientConfig {
            client_id: "assertion-machine".to_string(),
            client_secret: None,
            jwks: Some(client_assertion_jwks()),
            scopes: vec!["lab".to_string()],
            resources: vec!["https://lab.example.com/mcp".to_string()],
        }
    }

    async fn machine_client_state(clients: Vec<MachineClientConfig>) -> AuthState {
        let mut config = test_auth_config();
        config.machine_clients = clients;
        test_auth_state_with_config(config).await
    }

    fn basic_authorization(client_id: &str, client_secret: &str) -> String {
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD
                .encode(format!("{client_id}:{client_secret}"))
        )
    }

    async fn post_token(
        state: &AuthState,
        body: String,
        authorization: Option<String>,
    ) -> (StatusCode, serde_json::Value) {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/token")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
        if let Some(authorization) = authorization {
            builder = builder.header(header::AUTHORIZATION, authorization);
        }
        let response = router(state.clone())
            .oneshot(builder.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    #[tokio::test]
    async fn client_credentials_grant_accepts_client_secret_basic() {
        let state = machine_client_state(vec![secret_machine_client()]).await;
        let (status, json) = post_token(
            &state,
            "grant_type=client_credentials".to_string(),
            Some(basic_authorization("machine", "machine-secret")),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{json}");
        assert_eq!(json["scope"], "lab");
        assert!(
            json.get("refresh_token").is_none(),
            "machine grants must not mint refresh tokens: {json}"
        );
        let claims = insecure_decode::<crate::jwt::AccessClaims>(
            json["access_token"].as_str().expect("access token"),
        )
        .expect("decode access token")
        .claims;
        assert_eq!(claims.sub, "machine");
        assert_eq!(claims.azp, "machine");
        assert_eq!(claims.aud, "https://lab.example.com/mcp");
    }

    #[tokio::test]
    async fn client_credentials_grant_rejects_a_wrong_client_secret() {
        let state = machine_client_state(vec![secret_machine_client()]).await;
        let (status, json) = post_token(
            &state,
            "grant_type=client_credentials".to_string(),
            Some(basic_authorization("machine", "not-the-secret")),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(json["error"], "invalid_client");
    }

    #[tokio::test]
    async fn client_credentials_grant_rejects_basic_and_body_credentials_together() {
        let state = machine_client_state(vec![secret_machine_client()]).await;
        let (status, json) = post_token(
            &state,
            "grant_type=client_credentials&client_id=machine&client_secret=machine-secret"
                .to_string(),
            Some(basic_authorization("machine", "machine-secret")),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "ambiguous credentials must never be resolved in the client's favour: {json}"
        );
        assert_eq!(json["error"], "invalid_client");
    }

    #[tokio::test]
    async fn client_credentials_grant_rejects_scope_beyond_the_configured_grant() {
        let state = machine_client_state(vec![secret_machine_client()]).await;
        let (status, json) = post_token(
            &state,
            "grant_type=client_credentials&scope=lab%3Aadmin".to_string(),
            Some(basic_authorization("machine", "machine-secret")),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json["error"], "invalid_scope");
    }

    #[tokio::test]
    async fn client_credentials_grant_rejects_resource_beyond_the_configured_grant() {
        let state = machine_client_state(vec![secret_machine_client()]).await;
        let (status, json) = post_token(
            &state,
            "grant_type=client_credentials&resource=https%3A%2F%2Fother.example.com%2Fmcp"
                .to_string(),
            Some(basic_authorization("machine", "machine-secret")),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json["error"], "invalid_scope");
    }

    #[tokio::test]
    async fn client_credentials_grant_accepts_private_key_jwt_without_a_client_id_parameter() {
        let state = machine_client_state(vec![assertion_machine_client()]).await;
        let assertion = signed_client_assertion("assertion-machine", "assertion-jti-1");
        let (status, json) = post_token(
            &state,
            format!(
                "grant_type=client_credentials&client_assertion_type={}&client_assertion={assertion}",
                "urn%3Aietf%3Aparams%3Aoauth%3Aclient-assertion-type%3Ajwt-bearer"
            ),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{json}");
        let claims = insecure_decode::<crate::jwt::AccessClaims>(
            json["access_token"].as_str().expect("access token"),
        )
        .expect("decode access token")
        .claims;
        assert_eq!(claims.sub, "assertion-machine");
    }

    #[tokio::test]
    async fn client_credentials_grant_rejects_a_replayed_client_assertion() {
        let state = machine_client_state(vec![assertion_machine_client()]).await;
        let assertion = signed_client_assertion("assertion-machine", "replayed-jti");
        let body = format!(
            "grant_type=client_credentials&client_assertion_type={}&client_assertion={assertion}",
            "urn%3Aietf%3Aparams%3Aoauth%3Aclient-assertion-type%3Ajwt-bearer"
        );
        let (first, json) = post_token(&state, body.clone(), None).await;
        assert_eq!(first, StatusCode::OK, "{json}");
        let (replay, json) = post_token(&state, body, None).await;
        assert_eq!(
            replay,
            StatusCode::UNAUTHORIZED,
            "a client assertion jti must be single-use: {json}"
        );
        assert_eq!(json["error"], "invalid_client");
    }

    #[tokio::test]
    async fn client_credentials_grant_rejects_an_assertion_signed_by_an_unknown_key() {
        // Same claims, but the configured client authenticates with a shared
        // secret and publishes no JWKS at all.
        let state = machine_client_state(vec![secret_machine_client()]).await;
        let assertion = signed_client_assertion("machine", "foreign-key-jti");
        let (status, json) = post_token(
            &state,
            format!(
                "grant_type=client_credentials&client_assertion_type={}&client_assertion={assertion}",
                "urn%3Aietf%3Aparams%3Aoauth%3Aclient-assertion-type%3Ajwt-bearer"
            ),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(json["error"], "invalid_client");
    }

    #[tokio::test]
    async fn jwt_bearer_grant_issues_a_machine_token_from_its_assertion() {
        let state = machine_client_state(vec![assertion_machine_client()]).await;
        let assertion = signed_client_assertion("assertion-machine", "jwt-bearer-jti");
        let (status, json) = post_token(
            &state,
            format!(
                "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={assertion}"
            ),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{json}");
        let claims = insecure_decode::<crate::jwt::AccessClaims>(
            json["access_token"].as_str().expect("access token"),
        )
        .expect("decode access token")
        .claims;
        assert_eq!(claims.sub, "assertion-machine");
        assert!(json.get("refresh_token").is_none(), "{json}");
    }

    #[tokio::test]
    async fn jwt_bearer_grant_is_not_an_alias_for_client_credentials() {
        // Without an assertion the grant is rejected outright, so a machine
        // client cannot use it to launder a plain client_secret exchange past
        // whatever policy is attached to the JWT-bearer profile.
        let state = machine_client_state(vec![secret_machine_client()]).await;
        let (status, json) = post_token(
            &state,
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer".to_string(),
            Some(basic_authorization("machine", "machine-secret")),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json["error"], "invalid_request");
        assert_eq!(json["error_description"], "missing `assertion` parameter");
    }

    #[tokio::test]
    async fn jwt_bearer_grant_rejects_two_disagreeing_assertions() {
        let state = machine_client_state(vec![assertion_machine_client()]).await;
        let assertion = signed_client_assertion("assertion-machine", "grant-jti");
        let other = signed_client_assertion("assertion-machine", "credential-jti");
        let (status, json) = post_token(
            &state,
            format!(
                "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer\
                 &assertion={assertion}&client_assertion={other}"
            ),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(json["error"], "invalid_client");
    }

    #[tokio::test]
    async fn public_client_authorization_code_grant_is_unaffected_by_client_authentication() {
        // Regression guard for the whole point of `token_endpoint_auth_method
        // = "none"`: a public client still redeems its code with nothing but
        // PKCE, exactly as before client authentication was wired in.
        let state = test_auth_state_with_registered_client().await;
        seed_authorization_code(&state).await;
        let (status, json) = post_token(
            &state,
            "grant_type=authorization_code&code=lab-code&client_id=client\
             &redirect_uri=http://127.0.0.1:7777/callback&code_verifier=verifier"
                .to_string(),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{json}");
        assert!(json["access_token"].is_string());
    }

    #[tokio::test]
    async fn public_client_authorization_code_grant_tolerates_an_empty_client_secret_field() {
        // Clients that emit `client_secret=` instead of omitting it are still
        // public clients; a blank field must not be read as a presented secret.
        let state = test_auth_state_with_registered_client().await;
        seed_authorization_code(&state).await;
        let (status, json) = post_token(
            &state,
            "grant_type=authorization_code&code=lab-code&client_id=client&client_secret=\
             &redirect_uri=http://127.0.0.1:7777/callback&code_verifier=verifier"
                .to_string(),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{json}");
    }

    #[tokio::test]
    async fn public_client_authorization_code_grant_rejects_a_supplied_secret_without_burning_the_code()
     {
        let state = test_auth_state_with_registered_client().await;
        seed_authorization_code(&state).await;
        let (status, json) = post_token(
            &state,
            "grant_type=authorization_code&code=lab-code&client_id=client&client_secret=guess\
             &redirect_uri=http://127.0.0.1:7777/callback&code_verifier=verifier"
                .to_string(),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(json["error"], "invalid_client");

        // Client authentication runs before redemption, so the single-use code
        // must have survived the rejected attempt.
        let (status, json) = post_token(
            &state,
            "grant_type=authorization_code&code=lab-code&client_id=client\
             &redirect_uri=http://127.0.0.1:7777/callback&code_verifier=verifier"
                .to_string(),
            None,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "a failed client authentication must not consume the code: {json}"
        );
    }

    #[tokio::test]
    async fn refresh_grant_rejects_an_unregistered_client() {
        let state = test_auth_state_with_registered_client().await;
        state
            .store
            .upsert_refresh_token(crate::types::RefreshTokenRow {
                refresh_token: "refresh-token".to_string(),
                client_id: "ghost-client".to_string(),
                subject: "google-subject-123".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                provider_refresh_token: Some("provider-refresh".to_string()),
                created_at: crate::util::now_unix() - 60,
                expires_at: crate::util::now_unix() + 3600,
                token_endpoint_auth_method: None,
            })
            .await
            .unwrap();
        let (status, json) = post_token(
            &state,
            "grant_type=refresh_token&refresh_token=refresh-token&client_id=ghost-client"
                .to_string(),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(json["error"], "invalid_client");
    }

    /// `/token` must enforce its own rate limit. Client authentication runs
    /// ahead of grant work, and for a CIMD-shaped `client_id` that triggers an
    /// outbound metadata fetch - reachable with no valid authorization code.
    /// Unlimited, that is both a self-DoS and an unauthenticated trigger for
    /// outbound requests to attacker-chosen hosts. `check_token_rate_limit`
    /// existed and was documented for this endpoint but had no caller.
    #[tokio::test]
    async fn token_endpoint_is_rate_limited_after_configured_burst() {
        let mut config = test_auth_config();
        config.token_requests_per_minute = 1;
        let state = test_auth_state_with_config(config).await;
        let app = router(state);

        let body = "grant_type=authorization_code&code=nope&client_id=client\
                    &redirect_uri=http://127.0.0.1:7777/callback&code_verifier=v";

        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        // The grant itself fails (no such code); what matters is that the
        // request was admitted rather than throttled.
        assert_ne!(first.status(), StatusCode::TOO_MANY_REQUESTS);

        let second = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/token")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    // --- recorded token_endpoint_auth_method ----------------------------
    //
    // A CIMD `client_id` whose metadata host cannot be reached. Resolving it
    // is a DNS failure, so any code path that re-resolves this client at
    // `/token` fails; a path that authenticates from the grant's recorded
    // method does not. That contrast is what these tests measure.
    const UNREACHABLE_CIMD_CLIENT: &str = "https://unreachable-client.invalid/client.json";

    fn form_encoded(value: &str) -> String {
        url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
    }

    /// Seed a refresh token issued to `client_id` and recorded as having been
    /// granted under `method`. `None` is a row written before schema v5
    /// started recording one.
    async fn seed_recorded_refresh_token(
        state: &AuthState,
        refresh_token: &str,
        client_id: &str,
        method: Option<&str>,
    ) {
        state
            .store
            .upsert_refresh_token(crate::types::RefreshTokenRow {
                refresh_token: refresh_token.to_string(),
                client_id: client_id.to_string(),
                subject: "google-subject-123".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                provider_refresh_token: Some("provider-refresh".to_string()),
                created_at: crate::util::now_unix() - 60,
                expires_at: crate::util::now_unix() + 3600,
                token_endpoint_auth_method: method.map(str::to_string),
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn refresh_grant_for_a_recorded_public_client_survives_an_unreachable_metadata_host() {
        // The bug this fixes: wiring client authentication into `/token` made
        // every refresh re-resolve the client, which for a CIMD `client_id` is
        // a live metadata fetch. A valid, unrevoked refresh token then failed
        // for as long as the client's own metadata host was down. A public
        // client presents no credentials to check against that document, so
        // the recorded method is enough and no fetch happens.
        let state = test_auth_state_with_mock_google().await;
        seed_recorded_refresh_token(
            &state,
            "public-cimd-token",
            UNREACHABLE_CIMD_CLIENT,
            Some("none"),
        )
        .await;
        let (status, json) = post_token(
            &state,
            format!(
                "grant_type=refresh_token&refresh_token=public-cimd-token&client_id={}",
                form_encoded(UNREACHABLE_CIMD_CLIENT)
            ),
            None,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "a public client's refresh must not depend on its metadata host: {json}"
        );
        assert!(json["access_token"].is_string(), "{json}");
    }

    #[tokio::test]
    async fn refresh_grant_for_a_row_without_a_recorded_method_still_resolves_the_client() {
        // The legacy half of the same scenario: a row issued before schema v5
        // records nothing, so `/token` resolves the client exactly as it did
        // before - unchanged behaviour, including this failure. NULL must
        // never be read as "public".
        let state = test_auth_state_with_mock_google().await;
        seed_recorded_refresh_token(&state, "legacy-token", UNREACHABLE_CIMD_CLIENT, None).await;
        let (status, json) = post_token(
            &state,
            format!(
                "grant_type=refresh_token&refresh_token=legacy-token&client_id={}",
                form_encoded(UNREACHABLE_CIMD_CLIENT)
            ),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{json}");
        assert_eq!(json["error"], "invalid_request");
        assert!(
            json["error_description"]
                .as_str()
                .is_some_and(|description| description.contains("unreachable")),
            "the legacy path must still fail on client resolution: {json}"
        );
    }

    #[tokio::test]
    async fn refresh_grant_rejects_a_recorded_public_client_presenting_a_client_secret() {
        // The fast path skips the fetch, not the rule: a client registered
        // with `token_endpoint_auth_method = "none"` that presents credentials
        // is rejected exactly as it is when the client is resolved.
        let state = test_auth_state_with_mock_google().await;
        seed_recorded_refresh_token(
            &state,
            "public-secret-token",
            UNREACHABLE_CIMD_CLIENT,
            Some("none"),
        )
        .await;
        let (status, json) = post_token(
            &state,
            format!(
                "grant_type=refresh_token&refresh_token=public-secret-token\
                 &client_id={}&client_secret=guess",
                form_encoded(UNREACHABLE_CIMD_CLIENT)
            ),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{json}");
        assert_eq!(json["error"], "invalid_client");
    }

    #[tokio::test]
    async fn refresh_grant_rejects_a_recorded_public_client_presenting_a_client_assertion() {
        let state = test_auth_state_with_mock_google().await;
        seed_recorded_refresh_token(
            &state,
            "public-assertion-token",
            UNREACHABLE_CIMD_CLIENT,
            Some("none"),
        )
        .await;
        let assertion = signed_client_assertion(UNREACHABLE_CIMD_CLIENT, "public-assertion-jti");
        let (status, json) = post_token(
            &state,
            format!(
                "grant_type=refresh_token&refresh_token=public-assertion-token&client_id={}\
                 &client_assertion_type={}&client_assertion={assertion}",
                form_encoded(UNREACHABLE_CIMD_CLIENT),
                "urn%3Aietf%3Aparams%3Aoauth%3Aclient-assertion-type%3Ajwt-bearer"
            ),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{json}");
        assert_eq!(json["error"], "invalid_client");
    }

    /// A confidential client registered with `private_key_jwt`, whose JWKS
    /// lives in the registered-clients table.
    async fn state_with_private_key_jwt_client() -> AuthState {
        let state = test_auth_state_with_mock_google().await;
        state
            .store
            .register_client(crate::types::RegisteredClient {
                client_id: "jwt-client".to_string(),
                redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
                created_at: crate::util::now_unix(),
                token_endpoint_auth_method: "private_key_jwt".to_string(),
                jwks: Some(client_assertion_jwks()),
            })
            .await
            .unwrap();
        state
    }

    #[tokio::test]
    async fn refresh_grant_for_a_recorded_private_key_jwt_client_still_authenticates() {
        // Confidential clients are deliberately NOT short-circuited: their
        // JWKS must be read fresh because keys rotate, and they are presenting
        // an assertion anyway, so the resolution is inherent to the exchange.
        let state = state_with_private_key_jwt_client().await;
        seed_recorded_refresh_token(&state, "jwt-token", "jwt-client", Some("private_key_jwt"))
            .await;
        let assertion = signed_client_assertion("jwt-client", "refresh-assertion-jti");
        let (status, json) = post_token(
            &state,
            format!(
                "grant_type=refresh_token&refresh_token=jwt-token&client_id=jwt-client\
                 &client_assertion_type={}&client_assertion={assertion}",
                "urn%3Aietf%3Aparams%3Aoauth%3Aclient-assertion-type%3Ajwt-bearer"
            ),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{json}");
        assert!(json["access_token"].is_string(), "{json}");
    }

    #[tokio::test]
    async fn refresh_grant_for_a_recorded_private_key_jwt_client_rejects_a_missing_assertion() {
        let state = state_with_private_key_jwt_client().await;
        seed_recorded_refresh_token(
            &state,
            "jwt-bare-token",
            "jwt-client",
            Some("private_key_jwt"),
        )
        .await;
        let (status, json) = post_token(
            &state,
            "grant_type=refresh_token&refresh_token=jwt-bare-token&client_id=jwt-client"
                .to_string(),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{json}");
        assert_eq!(json["error"], "invalid_client");
    }

    #[tokio::test]
    async fn authorization_code_grant_carries_the_recorded_method_onto_the_refresh_token() {
        // The recorded method has to survive the hop from authorization code
        // to refresh token, or the very first refresh falls back to resolving
        // the client and the fix only lasts one exchange.
        let state = test_auth_state_with_registered_client().await;
        state
            .store
            .insert_auth_code(crate::types::AuthorizationCodeRow {
                code: "recorded-code".to_string(),
                client_id: "client".to_string(),
                subject: "google-subject-123".to_string(),
                redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                code_challenge: super::pkce_challenge("verifier"),
                code_challenge_method: "S256".to_string(),
                provider_refresh_token: Some("provider-refresh".to_string()),
                created_at: 1_700_000_000,
                expires_at: 4_102_444_800,
                token_endpoint_auth_method: Some("none".to_string()),
            })
            .await
            .unwrap();
        let (status, json) = post_token(
            &state,
            "grant_type=authorization_code&code=recorded-code&client_id=client\
             &redirect_uri=http://127.0.0.1:7777/callback&code_verifier=verifier"
                .to_string(),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{json}");
        let refresh_token = json["refresh_token"].as_str().expect("refresh token");
        let row = state
            .store
            .find_refresh_token(refresh_token)
            .await
            .unwrap()
            .expect("refresh token row");
        assert_eq!(row.token_endpoint_auth_method.as_deref(), Some("none"));
    }

    #[tokio::test]
    async fn refreshing_a_recorded_grant_preserves_its_method() {
        let state = test_auth_state_with_mock_google().await;
        seed_recorded_refresh_token(&state, "preserved-token", "client", Some("none")).await;
        let (status, json) = post_token(
            &state,
            "grant_type=refresh_token&refresh_token=preserved-token&client_id=client".to_string(),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{json}");
        let row = state
            .store
            .find_refresh_token("preserved-token")
            .await
            .unwrap()
            .expect("refresh token row");
        assert_eq!(
            row.token_endpoint_auth_method.as_deref(),
            Some("none"),
            "a refresh must not erase the contract the grant was issued under"
        );
    }

    #[tokio::test]
    async fn a_recorded_public_method_never_bypasses_a_configured_machine_client() {
        // A config-declared machine client authenticates against server
        // config, not a registration record. Even if a stored grant claims
        // `none` for that client id, its configured credentials still apply.
        let mut config = test_auth_config();
        config.machine_clients = vec![MachineClientConfig {
            client_id: "client".to_string(),
            client_secret: Some("machine-secret".to_string()),
            jwks: None,
            scopes: vec!["lab".to_string()],
            resources: vec!["https://lab.example.com/mcp".to_string()],
        }];
        let state = test_auth_state_with_config(config).await;
        seed_recorded_refresh_token(&state, "machine-shadowed-token", "client", Some("none")).await;
        let (status, json) = post_token(
            &state,
            "grant_type=refresh_token&refresh_token=machine-shadowed-token&client_id=client"
                .to_string(),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{json}");
        assert_eq!(json["error"], "invalid_client");
    }
}
