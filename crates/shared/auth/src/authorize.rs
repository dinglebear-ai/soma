use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect};
use axum::{Json, response::Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

use crate::error::AuthError;
use crate::oauth_provider::{AuthorizeUrlRequest, namespaced_subject};
use crate::registration::resolve_client_redirect_uris;
use crate::session::{append_set_cookie, build_browser_session_cookie, create_browser_session};
use crate::state::AuthState;
use crate::types::{
    AuthorizationCodeRow, AuthorizationRequestRow, AuthorizeQuery, BrowserLoginQuery,
    BrowserLoginStateRow, CallbackQuery, NativeAuthorizationResultRow, NativePollQuery,
    NativePollResponse,
};
use crate::util::{expires_at, fingerprint, now_unix, random_token, remote_ip};

const AUTH_REQUEST_TTL_SECS: i64 = 300;
const NATIVE_SUCCESS_PAGE: &str = r#"<!doctype html><html><body style="font-family:sans-serif;background:#07131c;color:#e6f4fb;text-align:center;padding-top:4rem"><h2>Signed in</h2><p>You can close this tab and return to the app.</p></body></html>"#;
const NATIVE_CALLBACK_EXPIRED_PAGE: &str = r#"<!doctype html><html><body style="font-family:sans-serif;background:#07131c;color:#e6f4fb;text-align:center;padding-top:4rem"><h2>Sign-in link expired</h2><p>Return to the app and start sign-in again.</p></body></html>"#;

/// Enforces the configured email allowlist.
///
/// `email_verified` is enforced before the email comparison: without this guard,
/// an attacker who presents someone else's unverified address through an
/// upstream provider could bypass the allowlist.
fn check_email_allowlist(
    provider_id: &str,
    email: Option<&str>,
    email_verified: Option<bool>,
    allowed_emails: &[String],
) -> Result<(), AuthError> {
    if allowed_emails.is_empty() {
        return Ok(());
    }
    if email_verified != Some(true) {
        warn!(
            provider = provider_id,
            "oauth callback rejected: provider did not return a verified email address"
        );
        return Err(AuthError::AuthFailed(format!(
            "{provider_id} did not return a verified email address"
        )));
    }
    let Some(e) = email else {
        warn!(
            provider = provider_id,
            "oauth callback rejected: provider did not return an email address"
        );
        return Err(AuthError::AuthFailed(format!(
            "{provider_id} did not return an email address"
        )));
    };
    let trimmed = e.trim();
    if allowed_emails
        .iter()
        .any(|a| a.eq_ignore_ascii_case(trimmed))
    {
        return Ok(());
    }
    warn!(
        provider = provider_id,
        email_id = %fingerprint(trimmed),
        "oauth callback rejected: email not in allowed list"
    );
    Err(AuthError::AuthFailed(
        "account is not permitted to access this gateway".to_string(),
    ))
}

pub async fn browser_login(
    State(state): State<AuthState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(query): Query<BrowserLoginQuery>,
) -> Result<Response, AuthError> {
    state.check_authorize_rate_limit(remote_ip(addr)).await?;
    let return_to = sanitize_return_to(&state, query.return_to.as_deref());

    let provider = match query.provider.as_deref() {
        Some(id) => state.provider(id)?,
        None if state.providers.len() > 1 => {
            return Ok(render_provider_picker(&state, &return_to));
        }
        None => state.provider_or_default(None)?,
    };

    state.ensure_pending_oauth_state_capacity().await?;
    let provider_code_verifier = random_token(32)?;
    let provider_code_challenge =
        URL_SAFE_NO_PAD.encode(Sha256::digest(provider_code_verifier.as_bytes()));
    let request_state = random_token(24)?;
    let oauth_state_id = fingerprint(&request_state);

    state
        .store
        .insert_browser_login_state(BrowserLoginStateRow {
            state: request_state.clone(),
            return_to: return_to.clone(),
            provider: provider.provider_id().to_string(),
            provider_code_verifier,
            created_at: now_unix(),
            expires_at: now_unix() + AUTH_REQUEST_TTL_SECS,
        })
        .await?;

    let location = provider.authorize_url(&AuthorizeUrlRequest {
        state: request_state,
        scope: state.config.default_scope.clone(),
        code_challenge: provider_code_challenge,
        code_challenge_method: "S256".to_string(),
        force_consent: true,
    })?;
    info!(
        oauth_state_id = %oauth_state_id,
        return_to = %return_to,
        provider = provider.provider_id(),
        "browser login redirected to upstream provider"
    );

    Ok((
        StatusCode::FOUND,
        [(header::LOCATION, location.to_string())],
    )
        .into_response())
}

/// Plain HTML provider-choice page shown by `browser_login` when the
/// deployment has more than one provider configured and the request did
/// not already say which one to use.
///
/// `return_to` is percent-encoded via `form_urlencoded` before
/// interpolation. Verified against `url::form_urlencoded::byte_serialize`'s
/// actual WHATWG `application/x-www-form-urlencoded` behavior: unreserved
/// bytes are alphanumeric plus `*-._`, space becomes `+`, everything else
/// (including `~`, which is NOT in the unreserved set for this encoder) is
/// percent-encoded — so the real output charset is `[A-Za-z0-9*\-._+%]`.
/// None of those characters can break out of a double-quoted HTML attribute,
/// so no separate HTML-escaping step is needed for `return_to_encoded`.
///
/// `provider_id`/`provider_label(...)` are interpolated as plain text (not
/// inside a percent-encoded query value), so they go through `html_escape`
/// as defense-in-depth even though every value that reaches this function
/// today is a hardcoded `&'static str` from a closed match — nothing about
/// the escaping costs anything, and it removes "provider names are always
/// compile-time literals" as a load-bearing invariant for this function's
/// XSS-safety.
fn render_provider_picker(state: &AuthState, return_to: &str) -> Response {
    let return_to_encoded: String =
        url::form_urlencoded::byte_serialize(return_to.as_bytes()).collect();
    let login_path = &state.config.login_path;
    let links: String = state
        .providers
        .values()
        .map(|provider| {
            let id = html_escape(provider.provider_id());
            let label = html_escape(provider_label(provider.provider_id()));
            format!(
                r#"<li><a href="{login_path}?provider={id}&return_to={return_to_encoded}">Sign in with {label}</a></li>"#
            )
        })
        .collect();
    let body = format!(
        r#"<!doctype html><html><body style="font-family:sans-serif;background:#07131c;color:#e6f4fb;text-align:center;padding-top:4rem"><h2>Sign in</h2><ul style="list-style:none;padding:0;font-size:1.1rem;line-height:2.5">{links}</ul></body></html>"#
    );
    let mut response = axum::response::Html(body).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn provider_label(provider_id: &str) -> &'static str {
    match provider_id {
        "google" => "Google",
        "authelia" => "Authelia",
        "github" => "GitHub",
        _ => "your identity provider",
    }
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub async fn authorize(
    State(state): State<AuthState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(query): Query<AuthorizeQuery>,
) -> Result<Response, AuthError> {
    state.check_authorize_rate_limit(remote_ip(addr)).await?;
    state.ensure_pending_oauth_state_capacity().await?;
    validate_response_type(&query.response_type)?;
    let resource = validate_resource(&state, query.resource.as_deref())?;
    let scope = validate_scope(&state, &resource, &query.scope)?;
    let client_state_id = fingerprint(&query.state);
    info!(
        client_id = %query.client_id,
        redirect_uri = %query.redirect_uri,
        client_state_id = %client_state_id,
        resource = %resource,
        requested_scope = %query.scope,
        normalized_scope = %scope,
        "oauth authorize request received"
    );
    let redirect_uris =
        resolve_client_redirect_uris(&state, &query.client_id, &client_state_id).await?;
    if !redirect_uris.iter().any(|uri| uri == &query.redirect_uri) {
        warn!(
            client_id = %query.client_id,
            redirect_uri = %query.redirect_uri,
            client_state_id = %client_state_id,
            "oauth authorize rejected: redirect URI does not match the registered/CIMD-allowlisted client"
        );
        return Err(AuthError::Validation(
            "redirect_uri does not match the registered client".to_string(),
        ));
    }
    if query.code_challenge_method != "S256" {
        warn!(
            client_id = %query.client_id,
            client_state_id = %client_state_id,
            code_challenge_method = %query.code_challenge_method,
            "oauth authorize rejected: unsupported PKCE method"
        );
        return Err(AuthError::Validation(
            "code_challenge_method must be S256".to_string(),
        ));
    }

    let provider = state.provider_or_default(query.provider.as_deref())?;
    let provider_code_verifier = random_token(32)?;
    let provider_code_challenge =
        URL_SAFE_NO_PAD.encode(Sha256::digest(provider_code_verifier.as_bytes()));
    let request_state = random_token(24)?;
    let oauth_state_id = fingerprint(&request_state);

    state
        .store
        .insert_authorization_request(AuthorizationRequestRow {
            state: request_state.clone(),
            client_id: query.client_id.clone(),
            redirect_uri: query.redirect_uri.clone(),
            client_state: query.state.clone(),
            resource: resource.clone(),
            scope: scope.clone(),
            provider: provider.provider_id().to_string(),
            provider_code_verifier,
            code_challenge: query.code_challenge.clone(),
            code_challenge_method: query.code_challenge_method.clone(),
            created_at: now_unix(),
            expires_at: now_unix() + AUTH_REQUEST_TTL_SECS,
        })
        .await?;

    // We don't know which upstream subject is about to sign in until they
    // come back from the consent screen, so use "has this gateway ever
    // minted a refresh token for THIS provider before" as a single-tenant
    // proxy for "already granted." Scoped per-provider (not global) —
    // otherwise a deployment where Google already has refresh tokens on file
    // would skip forced consent on a user's very first Authelia or GitHub
    // login, silently degrading that new provider's first session to no
    // local refresh token (caught in engineering review; see Task 8's
    // `has_any_refresh_token_for_provider`). Forcing full re-consent on
    // every DCR client attempt (Raycast, Warp, etc.) adds an interactive
    // round trip long enough for impatient clients to time out and retry
    // before the human finishes clicking through it.
    let force_consent = !state
        .store
        .has_any_refresh_token_for_provider(provider.provider_id())
        .await?;
    let location = provider.authorize_url(&AuthorizeUrlRequest {
        state: request_state,
        scope: scope.clone(),
        code_challenge: provider_code_challenge,
        code_challenge_method: "S256".to_string(),
        force_consent,
    })?;
    info!(
        client_id = %query.client_id,
        redirect_uri = %query.redirect_uri,
        client_state_id = %client_state_id,
        oauth_state_id = %oauth_state_id,
        resource = %resource,
        scope = %scope,
        provider = provider.provider_id(),
        "oauth authorize request redirected to upstream provider"
    );
    debug!(
        client_id = %query.client_id,
        oauth_state_id = %oauth_state_id,
        location = %location,
        "oauth authorize redirect URL generated"
    );

    Ok((
        StatusCode::FOUND,
        [(header::LOCATION, location.to_string())],
    )
        .into_response())
}

pub async fn callback(
    State(state): State<AuthState>,
    Query(query): Query<CallbackQuery>,
) -> Result<Response, AuthError> {
    let oauth_state_id = fingerprint(&query.state);
    info!(
        oauth_state_id = %oauth_state_id,
        "oauth callback received"
    );
    if let Some(login) = state.store.take_browser_login_state(&query.state).await? {
        let provider = state.provider(&login.provider)?;
        let exchange = provider
            .exchange_code(&query.code, &login.provider_code_verifier)
            .await?;
        let allowed = state.resolve_allowed_emails().await?;
        check_email_allowlist(
            provider.provider_id(),
            exchange.email.as_deref(),
            exchange.email_verified,
            &allowed,
        )?;
        let subject = namespaced_subject(provider.provider_id(), &exchange.subject);
        let session = create_browser_session(&state, subject, exchange.email).await?;
        let mut response = Redirect::to(&login.return_to).into_response();
        append_set_cookie(
            &mut response,
            &build_browser_session_cookie(&state, &session.session_id),
        );
        info!(
            oauth_state_id = %oauth_state_id,
            return_to = %login.return_to,
            subject_id = %fingerprint(&session.subject),
            provider = provider.provider_id(),
            "browser login callback issued session cookie"
        );
        return Ok(response);
    }

    let request = state
        .store
        .take_authorization_request(&query.state)
        .await
        .map_err(|_| {
            warn!(
                oauth_state_id = %oauth_state_id,
                "oauth callback rejected: authorization state is invalid or expired"
            );
            AuthError::InvalidGrant("authorization state is invalid or expired".to_string())
        })?;
    info!(
        client_id = %request.client_id,
        redirect_uri = %request.redirect_uri,
        oauth_state_id = %oauth_state_id,
        client_state_id = %fingerprint(&request.client_state),
        resource = %request.resource,
        scope = %request.scope,
        "oauth callback state redeemed"
    );
    let provider = state.provider(&request.provider)?;
    let exchange = provider
        .exchange_code(&query.code, &request.provider_code_verifier)
        .await?;

    // RFC 9207: echo the issuer identifier on the authorization response (both
    // success and error) so the client can detect authorization-server mix-up
    // attacks. Matches the `issuer` advertised in authorization-server metadata.
    let issuer = crate::metadata::public_base_url(&state);

    // RFC 6749 §4.1.2.1: errors must redirect to the client's redirect_uri,
    // not surface as a JSON HTTP error. The denial reason is sourced from the
    // AuthError so we only log once (inside check_email_allowlist).
    let allowed = state.resolve_allowed_emails().await?;
    if let Err(denial) = check_email_allowlist(
        provider.provider_id(),
        exchange.email.as_deref(),
        exchange.email_verified,
        &allowed,
    ) {
        let mut redirect_target = url::Url::parse(&request.redirect_uri).map_err(|error| {
            // Unreachable in practice: redirect_uri was validated against the
            // client's registered URIs before being stored.
            AuthError::Config(format!("failed to parse registered redirect_uri: {error}"))
        })?;
        redirect_target
            .query_pairs_mut()
            .append_pair("error", "access_denied")
            .append_pair("error_description", &denial.to_string())
            .append_pair("state", &request.client_state)
            .append_pair("iss", &issuer);
        return Ok(Redirect::to(redirect_target.as_str()).into_response());
    }

    let subject = namespaced_subject(provider.provider_id(), &exchange.subject);
    let subject_id = fingerprint(&subject);
    info!(
        client_id = %request.client_id,
        oauth_state_id = %oauth_state_id,
        subject_id = %subject_id,
        provider = provider.provider_id(),
        has_provider_refresh_token = exchange.refresh_token.is_some(),
        "oauth callback exchanged upstream code successfully"
    );
    let auth_code = random_token(24)?;
    let auth_code_id = fingerprint(&auth_code);
    // The user just passed `check_email_allowlist`, which IS the admin gate:
    // operators are added to the allowlist explicitly to grant access. Elevate
    // their scope to include `<default_scope>:admin` so MCP clients (which
    // typically don't know to request elevated scopes) can call destructive
    // gateway/setup actions without a separate flow. If they explicitly
    // requested only the base scope, this is a no-op deny — they get admin.
    let elevated_scope =
        elevate_scope_for_allowed_user(&request.scope, &state.config.default_scope);
    let request_client_id = request.client_id.clone();
    let request_resource = request.resource.clone();
    let request_scope = elevated_scope.clone();
    state
        .store
        .insert_auth_code(AuthorizationCodeRow {
            code: auth_code.clone(),
            client_id: request.client_id,
            subject,
            redirect_uri: request.redirect_uri.clone(),
            resource: request.resource,
            scope: elevated_scope,
            provider: provider.provider_id().to_string(),
            code_challenge: request.code_challenge,
            code_challenge_method: request.code_challenge_method,
            provider_refresh_token: exchange.refresh_token,
            created_at: now_unix(),
            expires_at: expires_at(
                now_unix(),
                state.config.auth_code_ttl,
                &format!("{}_AUTH_CODE_TTL_SECS", state.config.env_prefix),
            )?,
        })
        .await?;
    info!(
        auth_code_id = %auth_code_id,
        oauth_state_id = %oauth_state_id,
        client_id = %request_client_id,
        resource = %request_resource,
        scope = %request_scope,
        redirect_uri = %request.redirect_uri,
        "oauth callback issued local authorization code"
    );

    // Native-flow clients (desktop/mobile apps with no loopback listener or
    // custom URI scheme) register `redirect_uri = native_callback_endpoint` —
    // our own HTTPS route — instead of a client-hosted URL. In that case there
    // is no redirect target to send the browser back to: stash the code keyed
    // by `state` for the client to retrieve via `/native/poll`, and show a
    // plain "signed in" page directly.
    let native_callback_endpoint = crate::metadata::native_callback_endpoint(&state);
    if request.redirect_uri == native_callback_endpoint {
        let now = now_unix();
        state
            .store
            .insert_native_authorization_result(NativeAuthorizationResultRow {
                state: request.client_state,
                code: auth_code,
                created_at: now,
                expires_at: expires_at(
                    now,
                    state.config.auth_code_ttl,
                    &format!("{}_AUTH_CODE_TTL_SECS", state.config.env_prefix),
                )?,
            })
            .await?;
        let mut response = axum::response::Html(NATIVE_SUCCESS_PAGE).into_response();
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        debug!(
            auth_code_id = %auth_code_id,
            native_callback_endpoint = %native_callback_endpoint,
            "oauth callback stored native authorization code for polling"
        );
        return Ok(response);
    }

    let redirect_uri = reqwest::Url::parse(&request.redirect_uri).map_err(|error| {
        AuthError::Storage(format!(
            "registered redirect_uri is not a valid URL: {error}"
        ))
    })?;
    let mut redirect_uri = redirect_uri;
    redirect_uri
        .query_pairs_mut()
        .append_pair("code", &auth_code)
        .append_pair("state", &request.client_state)
        .append_pair("iss", &issuer);
    debug!(
        auth_code_id = %auth_code_id,
        redirect_uri = %redirect_uri,
        "oauth callback redirecting client back to registered callback"
    );

    Ok(Redirect::to(redirect_uri.as_str()).into_response())
}

/// Direct-hit fallback for the registered native `redirect_uri`. In the real
/// flow this path is never dereferenced by an actual browser redirect — the
/// upstream provider's redirect target is always its own
/// `/auth/<provider>/callback` (e.g. `/auth/google/callback`,
/// `/auth/github/callback`), which detects a native-flow authorization
/// request and short-circuits into stashing the code for `/native/poll`
/// instead of redirecting here. This handler only answers a stray direct
/// visit (e.g. a stale bookmark or a misconfigured client), so `state` is
/// validated for URL-shape consistency but deliberately not looked up —
/// there's nothing to correlate it against.
pub async fn native_callback(Query(query): Query<NativePollQuery>) -> Result<Response, AuthError> {
    let state_param = query.state.trim();
    if state_param.is_empty() {
        return Err(AuthError::Validation(
            "missing `state` parameter".to_string(),
        ));
    }
    let mut response = (
        StatusCode::GONE,
        axum::response::Html(NATIVE_CALLBACK_EXPIRED_PAGE),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

pub async fn native_poll(
    State(state): State<AuthState>,
    Query(query): Query<NativePollQuery>,
) -> Result<Response, AuthError> {
    let state_param = query.state.trim();
    if state_param.is_empty() {
        return Err(AuthError::Validation(
            "missing `state` parameter".to_string(),
        ));
    }
    let mut response = if let Some(row) = state
        .store
        .take_native_authorization_result(state_param)
        .await?
    {
        Json(NativePollResponse {
            code: Some(row.code),
        })
        .into_response()
    } else {
        (
            StatusCode::ACCEPTED,
            Json(NativePollResponse { code: None }),
        )
            .into_response()
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

fn sanitize_return_to(state: &AuthState, requested: Option<&str>) -> String {
    let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) else {
        return "/".to_string();
    };
    if requested.starts_with('/') && !requested.starts_with("//") {
        return requested.to_string();
    }
    let Some(public_url) = state.config.public_url.as_ref() else {
        return "/".to_string();
    };
    let Ok(url) = reqwest::Url::parse(requested) else {
        return "/".to_string();
    };
    if url.scheme() != public_url.scheme()
        || url.host_str() != public_url.host_str()
        || url.port_or_known_default() != public_url.port_or_known_default()
    {
        return "/".to_string();
    }
    let mut normalized = url.path().to_string();
    if let Some(query) = url.query() {
        normalized.push('?');
        normalized.push_str(query);
    }
    if let Some(fragment) = url.fragment() {
        normalized.push('#');
        normalized.push_str(fragment);
    }
    normalized
}

fn validate_response_type(response_type: &str) -> Result<(), AuthError> {
    if response_type == "code" {
        Ok(())
    } else {
        warn!(
            response_type = %response_type,
            "oauth authorize rejected: unsupported response_type"
        );
        Err(AuthError::Validation(
            "response_type must be `code`".to_string(),
        ))
    }
}

/// Add `<base>:admin` to `scope` if not already present, where `base` is the
/// resource prefix of `default_scope` (everything before the first `:`).
///
/// For example, `default_scope = "syslog:read"` produces the admin scope
/// `"syslog:admin"`, not `"syslog:read:admin"`.
///
/// Called after `check_email_allowlist` succeeds. Being on the allowlist IS
/// the admin gate (operators add users explicitly), so the issued token
/// carries the elevated scope regardless of what the OAuth client originally
/// requested — most MCP clients use the default scope and have no way to
/// negotiate `:admin` themselves.
pub(crate) fn elevate_scope_for_allowed_user(scope: &str, default_scope: &str) -> String {
    let base = default_scope.split(':').next().unwrap_or(default_scope);
    let admin_scope = format!("{base}:admin");
    let mut scopes: Vec<&str> = scope.split_whitespace().filter(|s| !s.is_empty()).collect();
    // Always inject the default-brand admin scope (e.g. "lab:admin") for
    // allowlisted users, even when the token is for a cross-brand protected
    // route (e.g. "mcp:read mcp:write" for a cortex endpoint).  The JWT
    // audience is still bound to the specific resource URL, so a cortex token
    // carrying "lab:admin" cannot be presented to lab endpoints.  This lets
    // authenticate_protected_route_request recognise the admin unconditionally
    // without re-reading the allowlist at request time.
    if !scopes.contains(&admin_scope.as_str()) {
        scopes.push(admin_scope.as_str());
    }
    scopes.join(" ")
}

fn validate_scope(state: &AuthState, resource: &str, scope: &str) -> Result<String, AuthError> {
    let canonical = crate::metadata::canonical_resource_url(state);
    let supported = if resource.trim_end_matches('/') == canonical {
        state.config.scopes_supported.clone()
    } else {
        state
            .allowed_resource_scopes(resource)
            .filter(|scopes| !scopes.is_empty())
            .ok_or_else(|| {
                AuthError::Validation(format!(
                    "resource must be `{canonical}` or a configured protected MCP route"
                ))
            })?
    };
    let normalized = scope.trim();
    if normalized.is_empty() {
        if resource.trim_end_matches('/') == canonical {
            let scope = state.config.default_scope.clone();
            debug!(
                resource = %resource,
                scope = %scope,
                "oauth authorize defaulted scope"
            );
            return Ok(scope);
        }
        let scope = supported.join(" ");
        debug!(
            resource = %resource,
            scope = %scope,
            "oauth authorize defaulted protected resource scope"
        );
        return Ok(scope);
    }
    let requested = normalized.split_whitespace().collect::<Vec<_>>();
    if requested
        .iter()
        .all(|scope| supported.iter().any(|allowed| allowed == scope))
    {
        let scope = requested.join(" ");
        debug!(
            resource = %resource,
            requested_scope = %normalized,
            normalized_scope = %scope,
            "oauth authorize scope accepted"
        );
        return Ok(scope);
    }
    warn!(
        scope = %normalized,
        resource = %resource,
        supported_scopes = ?supported,
        "oauth authorize rejected: unsupported scope"
    );
    Err(AuthError::Validation(format!(
        "scope must be one of: {}",
        supported.join(", ")
    )))
}

pub(crate) fn validate_resource(
    state: &AuthState,
    requested: Option<&str>,
) -> Result<String, AuthError> {
    let canonical = crate::metadata::canonical_resource_url(state);
    let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(canonical);
    };
    let requested = requested.trim_end_matches('/');
    if requested == canonical || state.is_allowed_resource_url(requested) {
        debug!(
            requested_resource = %requested,
            canonical_resource = %canonical,
            protected_resource = requested != canonical,
            "oauth resource accepted"
        );
        return Ok(requested.to_string());
    }

    warn!(
        requested_resource = %requested,
        expected_resource = %canonical,
        "oauth request rejected: resource does not match an allowed MCP endpoint"
    );
    Err(AuthError::Validation(format!(
        "resource must be `{canonical}` or a configured protected MCP route"
    )))
}

#[cfg(test)]
pub mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use base64::Engine;
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use rsa::RsaPrivateKey;
    use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, LineEnding};
    use rsa::traits::PublicKeyParts;
    use serde_json::json;
    use tower::util::ServiceExt;
    use url::Url;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::config::{AuthConfig, AuthMode, GoogleConfig};
    use crate::error::AuthError;
    use crate::google::GoogleProvider;
    use crate::redirect_uri::{host_pattern_matches, is_allowed_redirect_uri, wildcard_matches};
    use crate::registration::{allowed_uris_from_cimd_document, allowlist_redirect_uris};
    use crate::state::AuthState;
    use crate::types::{AuthorizationRequestRow, NativeAuthorizationResultRow, RegisteredClient};

    use crate::util::now_unix;

    use axum::Router;
    use axum::extract::connect_info::MockConnectInfo;
    use std::net::SocketAddr;

    // `oneshot` bypasses the live `into_make_service_with_connect_info` layer,
    // so the rate-limit handlers' `ConnectInfo<SocketAddr>` extractor would be
    // missing and every request would 500. Wrap the real router with a mock
    // peer address; handlers that don't extract `ConnectInfo` ignore it.
    fn router(state: AuthState) -> Router {
        crate::routes::router(state)
            .layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 9001))))
    }

    #[test]
    fn allowlist_redirect_uris_keeps_only_patterns_that_pass_is_allowed_redirect_uri() {
        let candidates = vec![
            "http://127.0.0.1:7777/callback".to_string(), // loopback, always allowed
            "https://attacker.evil/steal-code".to_string(), // not in any allowlist pattern
            "https://callback.example.com/callback/node-a".to_string(), // matches pattern below
        ];
        let patterns = vec!["https://callback.example.com/callback/*".to_string()];
        let allowed = allowlist_redirect_uris(&candidates, &patterns);
        assert_eq!(
            allowed,
            vec![
                "http://127.0.0.1:7777/callback".to_string(),
                "https://callback.example.com/callback/node-a".to_string(),
            ]
        );
    }

    #[test]
    fn allowlist_redirect_uris_returns_empty_when_nothing_matches() {
        let candidates = vec!["https://attacker.evil/steal-code".to_string()];
        let allowed = allowlist_redirect_uris(&candidates, &[]);
        assert!(allowed.is_empty());
    }

    #[test]
    fn allowed_uris_from_cimd_document_returns_the_allowlisted_subset() {
        let document = crate::cimd::document::ClientMetadataDocument {
            client_id: "https://app.example.com/client.json".to_string(),
            client_name: "Example".to_string(),
            redirect_uris: vec![
                "http://127.0.0.1:3000/callback".to_string(),
                "https://attacker.evil/steal-code".to_string(),
            ],
        };
        let allowed = allowed_uris_from_cimd_document(
            &document,
            "https://app.example.com/client.json",
            "state-id",
            &[],
        )
        .expect("the loopback redirect_uri is allowed by default");
        assert_eq!(allowed, vec!["http://127.0.0.1:3000/callback".to_string()]);
    }

    #[test]
    fn allowed_uris_from_cimd_document_rejects_when_nothing_survives_the_allowlist() {
        // The whole point of filtering CIMD-declared redirect_uris through
        // the allowlist: a document that only declares an attacker-hosted
        // target must be rejected outright, not silently trusted.
        let document = crate::cimd::document::ClientMetadataDocument {
            client_id: "https://app.example.com/client.json".to_string(),
            client_name: "Example".to_string(),
            redirect_uris: vec!["https://attacker.evil/steal-code".to_string()],
        };
        let err = allowed_uris_from_cimd_document(
            &document,
            "https://app.example.com/client.json",
            "state-id",
            &[],
        )
        .unwrap_err();
        assert!(matches!(err, AuthError::Validation(_)));
    }

    #[tokio::test]
    async fn authorize_rejects_a_cimd_client_id_that_targets_a_private_address() {
        // A `client_id` shaped like a CIMD URL but pointing at a private
        // address is rejected by the SSRF guard before any network I/O
        // happens -- this proves the full wire-up (is_cimd_client_id
        // routing, fetch_and_validate_client_metadata invocation, error
        // mapping) end to end via a real /authorize HTTP request, without
        // needing a reachable public HTTPS target.
        let app = router(test_auth_state().await);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/authorize?response_type=code&client_id=https://127.0.0.1/client.json&redirect_uri=http://127.0.0.1:7777/callback&state=abc&scope=lab&code_challenge=pkce&code_challenge_method=S256")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // The response must carry only the generic message -- the detailed
        // CimdError (which would reveal "resolved only to a private
        // address" and leak internal-network-topology information to an
        // anonymous caller) must never appear in the HTTP response body.
        assert_eq!(
            json["message"],
            "client_id metadata document is invalid or unreachable"
        );
        let raw_body = String::from_utf8(body.to_vec()).unwrap();
        assert!(!raw_body.contains("ssrf_blocked"), "{raw_body}");
        assert!(!raw_body.contains("127.0.0.1"), "{raw_body}");
        assert!(!raw_body.contains("private"), "{raw_body}");
    }

    #[tokio::test]
    async fn register_accepts_public_dcr_and_enforces_loopback_redirects() {
        let mut config = test_auth_config();
        config.enable_dynamic_registration = true;
        let app = router(test_auth_state_with_config(config).await);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/register")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "redirect_uris": ["http://127.0.0.1:7777/callback"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let rejected = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/register")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "redirect_uris": ["https://claude.ai/api/mcp/auth_callback"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn register_accepts_native_callback_endpoint_without_redirect_allowlist() {
        let mut config = test_auth_config();
        config.enable_dynamic_registration = true;
        let state = test_auth_state_with_config(config).await;
        let native_callback_endpoint = crate::metadata::native_callback_endpoint(&state);
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/register")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "redirect_uris": [native_callback_endpoint] }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn register_rejects_native_callback_endpoint_smuggled_with_an_unsafe_redirect_uri() {
        // The native-endpoint bypass in `register_client` is per-redirect_uri —
        // confirm a registration that mixes the native endpoint with an
        // otherwise-disallowed redirect_uri in the same request still fails
        // validation for the whole request, rather than the native match
        // short-circuiting the loop and letting the unsafe URI through.
        let mut config = test_auth_config();
        config.enable_dynamic_registration = true;
        let state = test_auth_state_with_config(config).await;
        let native_callback_endpoint = crate::metadata::native_callback_endpoint(&state);
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/register")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "redirect_uris": [
                                native_callback_endpoint,
                                "https://evil.example/callback",
                            ]
                        })
                        .to_string(),
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
        assert_eq!(
            json.get("error").and_then(|v| v.as_str()),
            Some("invalid_redirect_uri"),
            "must use the RFC 7591 error/error_description shape: {json}"
        );
        assert!(
            json.get("error_description")
                .and_then(|v| v.as_str())
                .is_some_and(|s| !s.is_empty()),
            "error_description must be present and non-empty: {json}"
        );
        assert!(json.get("kind").is_none());
        assert!(json.get("message").is_none());
    }

    #[tokio::test]
    async fn register_accepts_and_echoes_application_type() {
        let mut config = test_auth_config();
        config.enable_dynamic_registration = true;
        let app = router(test_auth_state_with_config(config).await);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/register")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "redirect_uris": ["http://127.0.0.1:7777/callback"],
                            "application_type": "native"
                        })
                        .to_string(),
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
        assert_eq!(
            json.get("application_type").and_then(|v| v.as_str()),
            Some("native"),
            "DCR response must echo the registered application_type: {json}"
        );
    }

    #[tokio::test]
    async fn register_defaults_application_type_to_web_when_absent() {
        let mut config = test_auth_config();
        config.enable_dynamic_registration = true;
        let app = router(test_auth_state_with_config(config).await);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/register")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "redirect_uris": ["http://127.0.0.1:7777/callback"]
                        })
                        .to_string(),
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
        assert_eq!(
            json.get("application_type").and_then(|v| v.as_str()),
            Some("web"),
            "absent application_type must default to web (OIDC default): {json}"
        );
    }

    #[tokio::test]
    async fn register_rejects_invalid_application_type() {
        let mut config = test_auth_config();
        config.enable_dynamic_registration = true;
        let app = router(test_auth_state_with_config(config).await);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/register")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "redirect_uris": ["http://127.0.0.1:7777/callback"],
                            "application_type": "bogus"
                        })
                        .to_string(),
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
        assert_eq!(
            json.get("error").and_then(|v| v.as_str()),
            Some("invalid_client_metadata"),
            "must use the RFC 7591 error/error_description shape: {json}"
        );
        assert!(
            json.get("error_description")
                .and_then(|v| v.as_str())
                .is_some_and(|s| !s.is_empty()),
            "error_description must be present and non-empty: {json}"
        );
        assert!(json.get("kind").is_none());
        assert!(json.get("message").is_none());
    }

    #[tokio::test]
    async fn native_poll_returns_202_with_no_code_for_an_unknown_state() {
        let app = router(test_auth_state().await);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/native/poll?state=never-issued")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("code").is_none());
    }

    #[tokio::test]
    async fn native_poll_rejects_missing_state() {
        let app = router(test_auth_state().await);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/native/poll?state=")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn native_poll_is_one_shot_and_returns_the_code_exactly_once() {
        let state = test_auth_state().await;
        state
            .store
            .insert_native_authorization_result(NativeAuthorizationResultRow {
                state: "poll-me".to_string(),
                code: "the-code".to_string(),
                created_at: now_unix(),
                expires_at: now_unix() + 300,
            })
            .await
            .unwrap();
        let app = router(state);

        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/native/poll?state=poll-me")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        let body = axum::body::to_bytes(first.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "the-code");

        // Second poll for the same `state` must not still return the code —
        // `take_native_authorization_result` is a one-shot read-and-delete.
        let second = app
            .oneshot(
                Request::builder()
                    .uri("/native/poll?state=poll-me")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn native_callback_direct_hit_shows_expired_page_and_never_stores_a_code() {
        let app = router(test_auth_state().await);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/native/callback?state=whatever&code=attacker-supplied")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::GONE);
    }

    #[tokio::test]
    async fn insert_native_authorization_result_overwrites_on_state_collision() {
        // A retried /authorize with a reused `state` must not silently lose
        // the newer code — last-write-wins, not `DO NOTHING`.
        let state = test_auth_state().await;
        state
            .store
            .insert_native_authorization_result(NativeAuthorizationResultRow {
                state: "collide".to_string(),
                code: "first-code".to_string(),
                created_at: now_unix(),
                expires_at: now_unix() + 300,
            })
            .await
            .unwrap();
        state
            .store
            .insert_native_authorization_result(NativeAuthorizationResultRow {
                state: "collide".to_string(),
                code: "second-code".to_string(),
                created_at: now_unix(),
                expires_at: now_unix() + 300,
            })
            .await
            .unwrap();
        let fetched = state
            .store
            .take_native_authorization_result("collide")
            .await
            .unwrap()
            .expect("row should still be present");
        assert_eq!(fetched.code, "second-code");
    }

    #[tokio::test]
    async fn callback_stores_native_flow_code_for_polling_instead_of_redirecting() {
        let native_state = test_auth_state_with_mock_google_native().await;
        let app = router(native_state);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/auth/google/callback?state=native-good-state&code=upstream-code")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // The native branch never redirects the browser — it shows a static
        // "signed in" page directly.
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("Signed in"));

        let poll = app
            .oneshot(
                Request::builder()
                    .uri("/native/poll?state=native-client-state")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(poll.status(), StatusCode::OK);
        let poll_body = axum::body::to_bytes(poll.into_body(), usize::MAX)
            .await
            .unwrap();
        let poll_json: serde_json::Value = serde_json::from_slice(&poll_body).unwrap();
        assert!(poll_json["code"].as_str().is_some());
    }

    #[tokio::test]
    async fn register_accepts_allowed_non_loopback_redirect_patterns() {
        let mut config = test_auth_config();
        config.enable_dynamic_registration = true;
        config.allowed_client_redirect_uris =
            vec!["https://callback.example.com/callback/*".to_string()];
        let app = router(test_auth_state_with_config(config).await);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/register")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "redirect_uris": ["https://callback.example.com/callback/node-a"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn register_is_rate_limited_after_configured_burst() {
        let mut config = test_auth_config();
        config.enable_dynamic_registration = true;
        config.register_requests_per_minute = 1;
        let app = router(test_auth_state_with_config(config).await);

        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/register")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "redirect_uris": ["http://127.0.0.1:7777/callback"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);

        let second = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/register")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "redirect_uris": ["http://127.0.0.1:8888/callback"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn wildcard_redirect_patterns_support_leading_and_infix_matches() {
        assert!(wildcard_matches(
            "https://callback.example.com/callback/*",
            "https://callback.example.com/callback/node-a"
        ));
        assert!(wildcard_matches(
            "https://callback.*.com/callback/*",
            "https://callback.example.com/callback/node-a"
        ));
        assert!(!wildcard_matches("/callback", "/callback/extra"));
    }

    #[test]
    fn host_patterns_support_full_label_wildcards_only() {
        assert!(host_pattern_matches(
            "callback.*.com",
            "callback.example.com"
        ));
        assert!(host_pattern_matches(
            "*.example.com",
            "callback.example.com"
        ));
        assert!(!host_pattern_matches(
            "callback.example.com*",
            "callback.example.com"
        ));
        assert!(!host_pattern_matches(
            "*.example.com",
            "callback.nested.example.com"
        ));
    }

    #[test]
    fn wildcard_redirect_patterns_do_not_overmatch_similar_hosts() {
        assert!(!is_allowed_redirect_uri(
            "https://callback.example.com.evil.example/callback/node-a",
            &[String::from("https://callback.example.com/callback/*")]
        ));
        assert!(!is_allowed_redirect_uri(
            "https://callback.example.com.evil.example/callback",
            &[String::from("https://callback.example.com*")]
        ));
    }

    #[test]
    fn native_app_scheme_redirect_uris_are_always_allowed() {
        // Native-app redirects (RFC 8252 §7.1) like `com.raycast:/oauth` or
        // `warp://mcp/oauth2callback` are scoped to whatever app the OS has
        // registered for that private-use scheme, so — like loopback — they
        // don't need a per-client allowlist entry.
        assert!(is_allowed_redirect_uri("com.raycast:/oauth", &[]));
        assert!(is_allowed_redirect_uri("warp://mcp/oauth2callback", &[]));
        assert!(is_allowed_redirect_uri(
            "com.raycast:/oauth",
            &[String::from("https://callback.example.com/callback/*")]
        ));
    }

    #[test]
    fn script_executing_pseudo_schemes_are_never_auto_allowed() {
        assert!(!is_allowed_redirect_uri("javascript:alert(1)", &[]));
        assert!(!is_allowed_redirect_uri("data:text/html,evil", &[]));
        assert!(!is_allowed_redirect_uri("file:///etc/passwd", &[]));
    }

    #[test]
    fn https_redirects_still_require_the_allowlist() {
        assert!(!is_allowed_redirect_uri(
            "https://evil.example/callback",
            &[String::from("https://callback.example.com/callback/*")]
        ));
        assert!(is_allowed_redirect_uri(
            "https://callback.example.com/callback/node-a",
            &[String::from("https://callback.example.com/callback/*")]
        ));
        assert!(is_allowed_redirect_uri(
            "https://chatgpt.com/connector/oauth/test-callback-id",
            &[String::from("https://chatgpt.com/connector/oauth/*")]
        ));
    }

    #[test]
    fn all_https_redirect_pattern_allows_any_https_callback_only() {
        assert!(is_allowed_redirect_uri(
            "https://gemini.google.com/mcp/oauth/callback",
            &[String::from("https://*")]
        ));
        assert!(is_allowed_redirect_uri(
            "https://example.deeply.nested.client.invalid/path/callback?state=ok",
            &[String::from("https://*")]
        ));
        assert!(!is_allowed_redirect_uri(
            "http://example.deeply.nested.client.invalid/path/callback",
            &[String::from("https://*")]
        ));
    }

    #[tokio::test]
    async fn authorize_persists_full_state_and_redirects_to_google() {
        let app = router(test_auth_state_with_registered_client().await);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/authorize?response_type=code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=abc&scope=lab&code_challenge=pkce&code_challenge_method=S256")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FOUND);
        let location = response
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(location.contains("accounts.google.com"));
        assert!(location.contains("prompt=consent"));
    }

    #[tokio::test]
    async fn authorize_omits_forced_consent_once_a_refresh_token_already_exists() {
        let state = test_auth_state_with_registered_client().await;
        state
            .store
            .upsert_refresh_token(crate::types::RefreshTokenRow {
                refresh_token: "existing-refresh".to_string(),
                client_id: "client".to_string(),
                subject: "google-user".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                provider_refresh_token: None,
                created_at: now_unix(),
                expires_at: now_unix() + 3600,
            })
            .await
            .unwrap();
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/authorize?response_type=code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=abc&scope=lab&code_challenge=pkce&code_challenge_method=S256")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FOUND);
        let location = response
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(location.contains("accounts.google.com"));
        assert!(!location.contains("prompt="));
    }

    #[tokio::test]
    async fn authorize_accepts_configured_protected_resource_scopes() {
        let state = test_auth_state_with_registered_client().await;
        state.set_allowed_resource_scopes([(
            "https://mcp.example.com/syslog".to_string(),
            vec!["mcp:read".to_string(), "mcp:write".to_string()],
        )]);
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/authorize?response_type=code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=abc&resource=https%3A%2F%2Fmcp.example.com%2Fsyslog&scope=mcp%3Aread%20mcp%3Awrite&code_challenge=pkce&code_challenge_method=S256")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FOUND);
    }

    #[tokio::test]
    async fn authorize_is_rate_limited_after_configured_burst() {
        let mut config = test_auth_config();
        config.authorize_requests_per_minute = 1;
        let state = test_auth_state_with_config(config).await;
        state
            .store
            .register_client(RegisteredClient {
                client_id: "client".to_string(),
                redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
                created_at: now_unix(),
            })
            .await
            .unwrap();
        let app = router(state);

        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/authorize?response_type=code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=abc&scope=lab&code_challenge=pkce&code_challenge_method=S256")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::FOUND);

        let second = app
            .oneshot(
                Request::builder()
                    .uri("/authorize?response_type=code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=def&scope=lab&code_challenge=pkce&code_challenge_method=S256")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn browser_login_starts_upstream_flow_and_persists_return_to_state() {
        let state = test_auth_state().await;
        let app = router(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/auth/login?return_to=%2Fgateways%2F%3Ftab%3Dlab")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FOUND);
        let location = Url::parse(
            response
                .headers()
                .get(header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap(),
        )
        .unwrap();
        let upstream_state = location
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.into_owned())
            .unwrap();
        let stored = state
            .store
            .take_browser_login_state(&upstream_state)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.return_to, "/gateways/?tab=lab");
    }

    #[tokio::test]
    async fn browser_login_rejects_when_pending_oauth_state_cap_is_reached() {
        let mut config = test_auth_config();
        config.max_pending_oauth_states = 1;
        let state = test_auth_state_with_config(config).await;
        state
            .store
            .insert_browser_login_state(crate::types::BrowserLoginStateRow {
                state: "existing-login".to_string(),
                return_to: "/".to_string(),
                provider: "google".to_string(),
                provider_code_verifier: "provider-verifier".to_string(),
                created_at: now_unix(),
                expires_at: now_unix() + 300,
            })
            .await
            .unwrap();

        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/auth/login?return_to=%2Fgateways%2F")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn callback_rejects_expired_or_mismatched_state() {
        let app = router(test_auth_state_with_mock_google().await);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/auth/google/callback?state=bad-state&code=upstream-code")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn browser_login_callback_sets_session_cookie_and_redirects_home() {
        let state = test_auth_state_with_mock_google().await;
        let app = router(state.clone());
        let login = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/auth/login?return_to=%2Fgateways%2F")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let location = Url::parse(
            login
                .headers()
                .get(header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap(),
        )
        .unwrap();
        let upstream_state = location
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.into_owned())
            .unwrap();

        let callback = app
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/auth/google/callback?state={upstream_state}&code=upstream-code"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(callback.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            callback.headers().get(header::LOCATION).unwrap(),
            "/gateways/"
        );
        let cookie = callback
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .find_map(|value| value.to_str().ok())
            .unwrap();
        assert!(cookie.contains("lab_session="));
    }

    #[tokio::test]
    async fn oauth_client_callback_redirects_with_access_denied_when_email_not_in_allowlist() {
        let mut config = test_auth_config();
        config.admin_email = "allowed@example.com".to_string();
        let base_state = test_auth_state_with_config(config).await;
        base_state
            .store
            .register_client(RegisteredClient {
                client_id: "client".to_string(),
                redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
                created_at: now_unix(),
            })
            .await
            .unwrap();
        // Pre-insert an authorization request (OAuth-client flow, not browser-login).
        base_state
            .store
            .insert_authorization_request(AuthorizationRequestRow {
                state: "good-state".to_string(),
                client_id: "client".to_string(),
                redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
                client_state: "client-abc".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                provider_code_verifier: "provider-verifier".to_string(),
                code_challenge: "challenge".to_string(),
                code_challenge_method: "S256".to_string(),
                created_at: now_unix(),
                expires_at: now_unix() + 300,
            })
            .await
            .unwrap();

        let server = Box::leak(Box::new(MockServer::start().await));
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "google-access-token",
                "refresh_token": "refresh-token",
                "expires_in": 3600,
                "id_token": signed_test_id_token(), // email=user@example.com, not in allowlist
            })))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/certs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(test_jwks()))
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
        )
        .with_jwks_endpoint(server.uri().parse::<Url>().unwrap().join("/certs").unwrap());

        let state = AuthState::for_tests(
            (*base_state.config).clone(),
            base_state.store.clone(),
            (*base_state.signing_keys).clone(),
            AuthState::google_only_providers(google),
        );
        let expected_iss = crate::metadata::public_base_url(&state);
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/auth/google/callback?state=good-state&code=upstream-code")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Must redirect (not 401) with error=access_denied and the original client state.
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let location = response
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        let redirect = Url::parse(location).unwrap();
        let params: std::collections::HashMap<_, _> = redirect.query_pairs().collect();
        assert_eq!(
            params.get("error").map(|v| v.as_ref()),
            Some("access_denied")
        );
        assert_eq!(params.get("state").map(|v| v.as_ref()), Some("client-abc"));
        assert_eq!(
            params.get("iss").map(|v| v.as_ref()),
            Some(expected_iss.as_str()),
            "RFC 9207 iss must be present on the error response: {location}"
        );
    }

    #[tokio::test]
    async fn browser_login_callback_rejects_email_not_in_allowlist() {
        let mut config = test_auth_config();
        // "allowed@example.com" is permitted; the mock id_token returns
        // "user@example.com" → callback must be denied with 401.
        config.admin_email = "allowed@example.com".to_string();
        let base_state = test_auth_state_with_config(config).await;
        base_state
            .store
            .register_client(RegisteredClient {
                client_id: "client".to_string(),
                redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
                created_at: now_unix(),
            })
            .await
            .unwrap();

        let server = Box::leak(Box::new(MockServer::start().await));
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "google-access-token",
                "refresh_token": "refresh-token",
                "expires_in": 3600,
                "id_token": signed_test_id_token(),
            })))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/certs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(test_jwks()))
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
        )
        .with_jwks_endpoint(server.uri().parse::<Url>().unwrap().join("/certs").unwrap());

        let state = AuthState::for_tests(
            (*base_state.config).clone(),
            base_state.store.clone(),
            (*base_state.signing_keys).clone(),
            AuthState::google_only_providers(google),
        );
        let app = router(state.clone());

        let login = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/auth/login?return_to=%2F")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let location = Url::parse(
            login
                .headers()
                .get(header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap(),
        )
        .unwrap();
        let upstream_state = location
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.into_owned())
            .unwrap();

        let callback = app
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/auth/google/callback?state={upstream_state}&code=upstream-code"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(callback.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn authorize_rejects_missing_or_invalid_response_type() {
        let app = router(test_auth_state_with_registered_client().await);
        for uri in [
            "/authorize?client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=abc&scope=lab&code_challenge=pkce&code_challenge_method=S256",
            "/authorize?response_type=token&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=abc&scope=lab&code_challenge=pkce&code_challenge_method=S256",
        ] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn validate_scope_accepts_supported_scopes_and_rejects_others() {
        let state = test_auth_state().await;
        let canonical = crate::metadata::canonical_resource_url(&state);
        // Empty scope falls back to configured default ("lab").
        assert_eq!(
            super::validate_scope(&state, &canonical, "").unwrap(),
            "lab"
        );
        // Base scope passes.
        assert_eq!(
            super::validate_scope(&state, &canonical, "lab").unwrap(),
            "lab"
        );
        // `:admin` is in `scopes_supported` by default — MCP clients can request
        // it explicitly. (Allowed-emails users also receive it implicitly via
        // elevate_scope_for_allowed_user at callback time.)
        assert_eq!(
            super::validate_scope(&state, &canonical, "lab:admin").unwrap(),
            "lab:admin"
        );
        // Anything not in scopes_supported is rejected.
        let err = super::validate_scope(&state, &canonical, "lab:write").unwrap_err();
        assert!(err.to_string().contains("lab"), "got: {err}");
    }

    #[test]
    fn elevate_scope_adds_admin_when_missing() {
        assert_eq!(
            super::elevate_scope_for_allowed_user("lab", "lab"),
            "lab lab:admin"
        );
        // Already has admin → no duplication.
        assert_eq!(
            super::elevate_scope_for_allowed_user("lab lab:admin", "lab"),
            "lab lab:admin"
        );
        // Empty scope → just admin (rare; OAuth default normally fills `lab`).
        assert_eq!(
            super::elevate_scope_for_allowed_user("", "lab"),
            "lab:admin"
        );
        // Different brand prefix (syslog, axon, etc.) uses its own default.
        assert_eq!(
            super::elevate_scope_for_allowed_user("syslog", "syslog"),
            "syslog syslog:admin"
        );
        // default_scope with verb suffix (e.g. syslog:read) → admin uses base prefix only,
        // not syslog:read:admin.
        assert_eq!(
            super::elevate_scope_for_allowed_user("syslog:read", "syslog:read"),
            "syslog:read syslog:admin"
        );
        // Already has correct admin even when default_scope carries a suffix.
        assert_eq!(
            super::elevate_scope_for_allowed_user("syslog:read syslog:admin", "syslog:read"),
            "syslog:read syslog:admin"
        );
        // Cross-brand: protected route token (mcp:read mcp:write) for a lab
        // default_scope gets lab:admin injected so authenticate_protected_route_request
        // can recognise the admin without re-reading the allowlist.
        assert_eq!(
            super::elevate_scope_for_allowed_user("mcp:read mcp:write", "lab"),
            "mcp:read mcp:write lab:admin"
        );
        // Cross-brand already has admin → no duplication.
        assert_eq!(
            super::elevate_scope_for_allowed_user("mcp:read mcp:write lab:admin", "lab"),
            "mcp:read mcp:write lab:admin"
        );
    }

    #[tokio::test]
    async fn authorize_rejects_invalid_scope() {
        let app = router(test_auth_state_with_registered_client().await);
        // `lab:write` is NOT in default scopes_supported; should be rejected.
        // (`lab:admin` IS in scopes_supported as of 2026-05; use a different
        // unsupported scope here.)
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/authorize?response_type=code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=abc&scope=lab:write&code_challenge=pkce&code_challenge_method=S256")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn authorize_rejects_mismatched_resource_parameter() {
        let app = router(test_auth_state_with_registered_client().await);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/authorize?response_type=code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=abc&resource=https://other.example.com/mcp&scope=lab&code_challenge=pkce&code_challenge_method=S256")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn callback_rejects_expired_state() {
        let state = test_auth_state_with_registered_client().await;
        state
            .store
            .insert_authorization_request(AuthorizationRequestRow {
                state: "expired-state".to_string(),
                client_id: "client".to_string(),
                redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
                client_state: "client-state".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                provider_code_verifier: "provider-verifier".to_string(),
                code_challenge: "challenge".to_string(),
                code_challenge_method: "S256".to_string(),
                created_at: now_unix() - 300,
                expires_at: now_unix() - 1,
            })
            .await
            .unwrap();
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/auth/google/callback?state=expired-state&code=upstream-code")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    pub async fn test_auth_state() -> AuthState {
        test_auth_state_with_config(test_auth_config()).await
    }

    pub async fn test_auth_state_with_config(config: AuthConfig) -> AuthState {
        AuthState::new(config).await.unwrap()
    }

    pub(crate) fn test_auth_config() -> AuthConfig {
        let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        AuthConfig {
            mode: AuthMode::OAuth,
            public_url: Some(Url::parse("https://lab.example.com").unwrap()),
            sqlite_path: dir.path().join("auth.db"),
            key_path: dir.path().join("auth-jwt.pem"),
            bootstrap_secret: Some("bootstrap-secret".to_string()),
            enable_dynamic_registration: true,
            allowed_client_redirect_uris: Vec::new(),
            // Matches the mock id_token email returned by signed_test_id_token,
            // so happy-path callback tests pass the allowlist check.
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
            default_provider: "google".to_string(),
            ..AuthConfig::default()
        }
    }

    pub async fn test_auth_state_with_registered_client() -> AuthState {
        let state = test_auth_state().await;
        state
            .store
            .register_client(RegisteredClient {
                client_id: "client".to_string(),
                redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
                created_at: now_unix(),
            })
            .await
            .unwrap();
        state
    }

    pub(crate) async fn test_auth_state_with_mock_google() -> AuthState {
        let state = test_auth_state_with_registered_client().await;
        let server = Box::leak(Box::new(MockServer::start().await));
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "google-access-token",
                "refresh_token": "refresh-token",
                "expires_in": 3600,
                "id_token": signed_test_id_token(),
            })))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/certs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(test_jwks()))
            .mount(server)
            .await;
        state
            .store
            .insert_authorization_request(AuthorizationRequestRow {
                state: "good-state".to_string(),
                client_id: "client".to_string(),
                redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
                client_state: "client-state".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                provider_code_verifier: "provider-verifier".to_string(),
                code_challenge: "challenge".to_string(),
                code_challenge_method: "S256".to_string(),
                created_at: now_unix(),
                expires_at: now_unix() + 300,
            })
            .await
            .unwrap();
        let google = GoogleProvider::new(
            "client-id".to_string(),
            "client-secret".to_string(),
            Url::parse("https://lab.example.com/auth/google/callback").unwrap(),
        )
        .unwrap()
        .with_endpoints(
            server.uri().parse::<Url>().unwrap(),
            server.uri().parse::<Url>().unwrap().join("/token").unwrap(),
        )
        .with_jwks_endpoint(server.uri().parse::<Url>().unwrap().join("/certs").unwrap());
        AuthState::for_tests(
            (*state.config).clone(),
            state.store.clone(),
            (*state.signing_keys).clone(),
            AuthState::google_only_providers(google),
        )
    }

    /// Same mocked-Google harness as [`test_auth_state_with_mock_google`], but
    /// the pending authorization request's `redirect_uri` is the server's own
    /// `native_callback_endpoint` — exercising the native-flow branch of
    /// `callback()` instead of the normal client-redirect branch.
    async fn test_auth_state_with_mock_google_native() -> AuthState {
        let state = test_auth_state().await;
        let native_callback_endpoint = crate::metadata::native_callback_endpoint(&state);
        state
            .store
            .register_client(RegisteredClient {
                client_id: "native-client".to_string(),
                redirect_uris: vec![native_callback_endpoint.clone()],
                created_at: now_unix(),
            })
            .await
            .unwrap();
        let server = Box::leak(Box::new(MockServer::start().await));
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "google-access-token",
                "refresh_token": "refresh-token",
                "expires_in": 3600,
                "id_token": signed_test_id_token(),
            })))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/certs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(test_jwks()))
            .mount(server)
            .await;
        state
            .store
            .insert_authorization_request(AuthorizationRequestRow {
                state: "native-good-state".to_string(),
                client_id: "native-client".to_string(),
                redirect_uri: native_callback_endpoint,
                client_state: "native-client-state".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                provider_code_verifier: "provider-verifier".to_string(),
                code_challenge: "challenge".to_string(),
                code_challenge_method: "S256".to_string(),
                created_at: now_unix(),
                expires_at: now_unix() + 300,
            })
            .await
            .unwrap();
        let google = GoogleProvider::new(
            "client-id".to_string(),
            "client-secret".to_string(),
            Url::parse("https://lab.example.com/auth/google/callback").unwrap(),
        )
        .unwrap()
        .with_endpoints(
            server.uri().parse::<Url>().unwrap(),
            server.uri().parse::<Url>().unwrap().join("/token").unwrap(),
        )
        .with_jwks_endpoint(server.uri().parse::<Url>().unwrap().join("/certs").unwrap());
        AuthState::for_tests(
            (*state.config).clone(),
            state.store.clone(),
            (*state.signing_keys).clone(),
            AuthState::google_only_providers(google),
        )
    }

    fn signed_test_id_token() -> String {
        let claims = json!({
            "iss": "https://accounts.google.com",
            "aud": "client-id",
            "sub": "google-subject-123",
            "email": "user@example.com",
            "email_verified": true,
            "iat": now_unix() as usize,
            "exp": (now_unix() + 3600) as usize,
        });
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-kid".to_string());
        encode(&header, &claims, &test_encoding_key()).unwrap()
    }

    fn test_jwks() -> serde_json::Value {
        let key = test_rsa_key();
        let public_key = key.to_public_key();
        json!({
            "keys": [{
                "kid": "test-kid",
                "alg": "RS256",
                "kty": "RSA",
                "use": "sig",
                "n": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public_key.n_bytes()),
                "e": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public_key.e_bytes()),
            }]
        })
    }

    fn test_rsa_key() -> RsaPrivateKey {
        RsaPrivateKey::from_pkcs8_pem(TEST_RSA_KEY_PEM).unwrap()
    }

    fn test_encoding_key() -> EncodingKey {
        let pem = test_rsa_key().to_pkcs8_pem(LineEnding::LF).unwrap();
        EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap()
    }

    const TEST_RSA_KEY_PEM: &str = r"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC/Wa3MQnrNbKu9
H5+ZH30lrKV3+EJeuY0ofx3qMx73ax+ArHaPFHXq3PUAalSZ+UlBqRmX89DdzwWG
l5hqt3wzGjGe49zxhY5+nUUPLtRiI4JH0iEH4Bg3W9e9gWAAPjVemuYmZ57R9XOd
O1l0aI20mZiy4jeEN7Ls40I/pwyTcB22krOeHz13E1NzG+uDQnaMZkOKomRdTkKr
tiSETBcpacpIdyLtdc9lHR4LbcZtBH3aMosjmgae3uvQyks6ntj0UQZaKNYqNwNE
+GSOqQdtJeoWhps1IYjhc9wcfrlL69nn5U4FXwCcPzGOKXCOW45/BB4nr2WF2Bkq
N7iytDv/AgMBAAECggEABt1BtdUgsKPYWVV8FTMi+yoBWZdnUhyX6r78pL0mvDt0
itok+qcCP+WjSFuII2nk7d0SFPhjIsHdceGYTyO76d1jsE5+S4+9997ObmgAqHCb
qNXp521rkPjTeXHdrsSMh5NI9FG9SczjU92gLOPfSX5FEw24bh7NZWAVrVDhy5wn
BWAZow2kByQ2SLRitUJr+a1xF3UO3PgHLKdP0H0qZp9TCar3nzJxwMUyGJxOcd4f
mElyYNIsJtOBsIIoBsNh+aj5pSjOiuEZmfipbHuMWpjEwF1+UVH4iPXQugyKgFze
Gc8wy3aFlmA4dH2jbSzP3aIwiFUDgqsUrqdyEXVVeQKBgQD5/psH3uk3AOkRC/k/
P6cI5pwFG0rFRe3UgBJFqODnbTZR+0BwyTqf9kCZgi0nJIudCNyUF5utl8rkWdwE
s2s42NibGWTVyb5dabT+dHwP42jFljCxxbZw1D3GmP1mX0ybyXj0BOqWEpMHc76q
ZxzJFfML0FfyTxMVycukBL4bEwKBgQDD8m2Y5GvO17RJDeG6yPupTvWbcBaUTuwe
0w9LOWSOYi3YPAIt7m6yE9XH9cWSFqXMoOAS5Lu1zUuBvwhZz3XAAeL9JpU2F/1V
DW7NiChNb7Np2X1dUHZTS5EmaAkok55uEMfA1N1FhsDfN+qCxVPITUszYwrPCu52
SMd4Nx5s5QKBgQDfK6woTZWyNYzaW+8IyIEL0BqN8HxCOZgD8MTfDNChqHwqmXpA
dVNxg3rNz0kRvW0pJcUMKzsdr/k++v0P8T+RwvszEmtS8sOPTpN16HTsFh3s7ZPQ
z2h7tuzjAqaMIh0YobXpWQ42JKS+rVQTePNYi9CpxjcMqAyokbnKVTWEowKBgFrB
5/eAHVsh19RahKoyOzZRZztGsH6jC4S/d379J1E3skpMiSnjHQyIWWWTtZ4TtVnR
TdgSb8smOonvBJwsljqH5S4h98ylUeZaIW87WId9bFljrkhRY2zzPFjQqSVNMn2C
cjMjpRV189GwIYPOiB7nhiRYBIKfapII5bMNvJ7tAoGACMvtonFh25b7gB7j3Pep
LEH/fA5CRiOs7Plrt2Sv54wAup4Y6+HQ8i/KFOXIejEN9vfY1YRfyD5Ajc05zg90
uE8aLb5YtFvoaLAnc/A2ceW8sNxGgT5aPyLPUdmfSryAO4ayFDHmRlGFRsZtTUbn
Iy60nwnOxK6B5mZV2Cs+kv8=
-----END PRIVATE KEY-----";

    /// Tests that exercise the merged allowlist path through real callback handlers.
    /// These verify that `resolve_allowed_emails` is correctly wired at both call
    /// sites (browser-login branch and oauth-client branch).
    mod merged_allowlist_callback_tests {
        use axum::body::Body;
        use axum::http::{Request, StatusCode, header};
        use serde_json::json;
        use tower::util::ServiceExt;
        use url::Url;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        use super::{
            signed_test_id_token, test_auth_config, test_auth_state_with_config,
            test_auth_state_with_mock_google, test_jwks,
        };
        use crate::google::GoogleProvider;
        use crate::routes::router;
        use crate::state::AuthState;
        use crate::types::{AuthorizationRequestRow, BrowserLoginStateRow, RegisteredClient};
        use crate::util::now_unix;

        /// Helper that mounts Google mock endpoints on a fresh server and builds
        /// an `AuthState` with that mock, reusing an existing base state's store
        /// and signing keys (so DB writes made to `base_state.store` are visible).
        async fn state_with_mock_google_from(base_state: &AuthState) -> AuthState {
            let server = Box::leak(Box::new(MockServer::start().await));
            Mock::given(method("POST"))
                .and(path("/token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "access_token": "google-access-token",
                    "refresh_token": "refresh-token",
                    "expires_in": 3600,
                    "id_token": signed_test_id_token(),
                })))
                .mount(server)
                .await;
            Mock::given(method("GET"))
                .and(path("/certs"))
                .respond_with(ResponseTemplate::new(200).set_body_json(test_jwks()))
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
            )
            .with_jwks_endpoint(server.uri().parse::<Url>().unwrap().join("/certs").unwrap());
            AuthState::for_tests(
                (*base_state.config).clone(),
                base_state.store.clone(),
                (*base_state.signing_keys).clone(),
                AuthState::google_only_providers(google),
            )
        }

        /// The mock id_token always returns `user@example.com`. When admin is set
        /// to a *different* email and that address is added to `allowed_users`, the
        /// browser-login callback must succeed (DB row authorises the login).
        #[tokio::test]
        async fn browser_login_succeeds_for_allowlisted_non_admin_email() {
            let mut config = test_auth_config();
            // Set admin to something other than the id_token email.
            config.admin_email = "admin@example.com".to_string();
            let base_state = test_auth_state_with_config(config).await;

            // Insert id_token email into allowed_users.
            base_state
                .store
                .add_allowed_user("user@example.com", "admin", now_unix())
                .await
                .unwrap();

            let state = state_with_mock_google_from(&base_state).await;

            // Seed the browser-login state row so the callback recognises the flow.
            state
                .store
                .insert_browser_login_state(BrowserLoginStateRow {
                    state: "browser-state".to_string(),
                    return_to: "/".to_string(),
                    provider: "google".to_string(),
                    provider_code_verifier: "provider-verifier".to_string(),
                    created_at: now_unix(),
                    expires_at: now_unix() + 300,
                })
                .await
                .unwrap();

            let app = router(state);
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/auth/google/callback?state=browser-state&code=upstream-code")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            // Successful browser login → redirect with a Set-Cookie header (session).
            assert_eq!(response.status(), StatusCode::SEE_OTHER);
            assert!(response.headers().contains_key(header::SET_COOKIE));
        }

        /// Admin email is always authorised even when the `allowed_users` table is
        /// empty (browser-login branch).
        #[tokio::test]
        async fn browser_login_succeeds_for_admin_when_allowed_users_is_empty() {
            // Default test config sets admin_email = "user@example.com", which
            // matches the id_token returned by signed_test_id_token.
            let base_state = test_auth_state_with_mock_google().await;

            // Confirm no extra rows exist.
            assert!(
                base_state
                    .store
                    .list_allowed_users()
                    .await
                    .unwrap()
                    .is_empty()
            );

            // Seed browser-login state.
            base_state
                .store
                .insert_browser_login_state(BrowserLoginStateRow {
                    state: "browser-state-2".to_string(),
                    return_to: "/".to_string(),
                    provider: "google".to_string(),
                    provider_code_verifier: "provider-verifier".to_string(),
                    created_at: now_unix(),
                    expires_at: now_unix() + 300,
                })
                .await
                .unwrap();

            let app = router(base_state);
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/auth/google/callback?state=browser-state-2&code=upstream-code")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::SEE_OTHER);
            assert!(response.headers().contains_key(header::SET_COOKIE));
        }

        /// The oauth-client callback must also succeed for a non-admin email that
        /// exists in `allowed_users`.
        #[tokio::test]
        async fn oauth_client_callback_succeeds_for_allowlisted_non_admin_email() {
            let mut config = test_auth_config();
            config.admin_email = "admin@example.com".to_string();
            let base_state = test_auth_state_with_config(config).await;

            // Register a client.
            base_state
                .store
                .register_client(RegisteredClient {
                    client_id: "client".to_string(),
                    redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
                    created_at: now_unix(),
                })
                .await
                .unwrap();

            // Add id_token email to allowed_users.
            base_state
                .store
                .add_allowed_user("user@example.com", "admin", now_unix())
                .await
                .unwrap();

            let state = state_with_mock_google_from(&base_state).await;

            // Seed an authorization request row.
            state
                .store
                .insert_authorization_request(AuthorizationRequestRow {
                    state: "oauth-state".to_string(),
                    client_id: "client".to_string(),
                    redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
                    client_state: "client-xyz".to_string(),
                    resource: "https://lab.example.com/mcp".to_string(),
                    scope: "lab".to_string(),
                    provider: "google".to_string(),
                    provider_code_verifier: "provider-verifier".to_string(),
                    code_challenge: "challenge".to_string(),
                    code_challenge_method: "S256".to_string(),
                    created_at: now_unix(),
                    expires_at: now_unix() + 300,
                })
                .await
                .unwrap();

            let app = router(state);
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/auth/google/callback?state=oauth-state&code=upstream-code")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            // Success: redirect to client callback with `code` param (no `error`).
            assert_eq!(response.status(), StatusCode::SEE_OTHER);
            let location = response
                .headers()
                .get(header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap();
            let redirect = Url::parse(location).unwrap();
            let params: std::collections::HashMap<_, _> = redirect.query_pairs().collect();
            assert!(
                params.contains_key("code"),
                "expected code in redirect: {location}"
            );
            assert!(
                !params.contains_key("error"),
                "unexpected error in redirect: {location}"
            );
        }

        /// Email not in admin or allowed_users must be rejected in the browser-login
        /// branch (401 Unauthorized).
        #[tokio::test]
        async fn browser_login_rejects_email_absent_from_both_admin_and_db() {
            let mut config = test_auth_config();
            // Neither admin nor allowed_users contains "user@example.com" (the id_token email).
            config.admin_email = "admin@example.com".to_string();
            let base_state = test_auth_state_with_config(config).await;

            let state = state_with_mock_google_from(&base_state).await;

            state
                .store
                .insert_browser_login_state(BrowserLoginStateRow {
                    state: "browser-state-3".to_string(),
                    return_to: "/".to_string(),
                    provider: "google".to_string(),
                    provider_code_verifier: "provider-verifier".to_string(),
                    created_at: now_unix(),
                    expires_at: now_unix() + 300,
                })
                .await
                .unwrap();

            let app = router(state);
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/auth/google/callback?state=browser-state-3&code=upstream-code")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }

        /// Admin also in the DB table must not appear twice (dedup check via
        /// resolve_allowed_emails, verified indirectly: the callback still succeeds
        /// and there is no panic from duplicate iteration).
        #[tokio::test]
        async fn admin_in_db_table_is_deduped_and_still_authorised() {
            // Default config: admin_email = "user@example.com".
            let base_state = test_auth_state_with_mock_google().await;

            // Also add the admin email to allowed_users — this is the duplicate.
            base_state
                .store
                .add_allowed_user("user@example.com", "self", now_unix())
                .await
                .unwrap();

            // Seed browser-login state.
            base_state
                .store
                .insert_browser_login_state(BrowserLoginStateRow {
                    state: "browser-state-4".to_string(),
                    return_to: "/".to_string(),
                    provider: "google".to_string(),
                    provider_code_verifier: "provider-verifier".to_string(),
                    created_at: now_unix(),
                    expires_at: now_unix() + 300,
                })
                .await
                .unwrap();

            let app = router(base_state);
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/auth/google/callback?state=browser-state-4&code=upstream-code")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            // Must still succeed — dedup should not break the check.
            assert_eq!(response.status(), StatusCode::SEE_OTHER);
            assert!(response.headers().contains_key(header::SET_COOKIE));
        }

        /// RFC 9207: the authorization success response MUST carry the `iss`
        /// parameter set to the authorization server's issuer identifier so the
        /// client can detect authorization-server mix-up attacks.
        #[tokio::test]
        async fn oauth_client_callback_includes_rfc9207_iss_on_success() {
            let mut config = test_auth_config();
            config.admin_email = "admin@example.com".to_string();
            let base_state = test_auth_state_with_config(config).await;

            base_state
                .store
                .register_client(RegisteredClient {
                    client_id: "client".to_string(),
                    redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
                    created_at: now_unix(),
                })
                .await
                .unwrap();
            base_state
                .store
                .add_allowed_user("user@example.com", "admin", now_unix())
                .await
                .unwrap();

            let state = state_with_mock_google_from(&base_state).await;
            state
                .store
                .insert_authorization_request(AuthorizationRequestRow {
                    state: "oauth-state".to_string(),
                    client_id: "client".to_string(),
                    redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
                    client_state: "client-xyz".to_string(),
                    resource: "https://lab.example.com/mcp".to_string(),
                    scope: "lab".to_string(),
                    provider: "google".to_string(),
                    provider_code_verifier: "provider-verifier".to_string(),
                    code_challenge: "challenge".to_string(),
                    code_challenge_method: "S256".to_string(),
                    created_at: now_unix(),
                    expires_at: now_unix() + 300,
                })
                .await
                .unwrap();

            let expected_iss = crate::metadata::public_base_url(&state);
            let app = router(state);
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/auth/google/callback?state=oauth-state&code=upstream-code")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::SEE_OTHER);
            let location = response
                .headers()
                .get(header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap();
            let redirect = Url::parse(location).unwrap();
            let params: std::collections::HashMap<_, _> = redirect.query_pairs().collect();
            assert_eq!(
                params.get("iss").map(|v| v.as_ref()),
                Some(expected_iss.as_str()),
                "RFC 9207 iss must equal the issuer identifier on success: {location}"
            );
        }
    }

    mod allowlist_tests {
        use super::super::check_email_allowlist;

        #[test]
        fn empty_allowlist_permits_any_email() {
            assert!(
                check_email_allowlist("google", Some("anyone@example.com"), Some(true), &[])
                    .is_ok()
            );
        }

        #[test]
        fn empty_allowlist_permits_even_unverified_email() {
            // When no allowlist is configured, email_verified is not enforced.
            assert!(
                check_email_allowlist("google", Some("anyone@example.com"), Some(false), &[])
                    .is_ok()
            );
        }

        #[test]
        fn matching_verified_email_is_permitted() {
            let list = vec!["alice@example.com".to_string()];
            assert!(
                check_email_allowlist("google", Some("alice@example.com"), Some(true), &list)
                    .is_ok()
            );
        }

        #[test]
        fn matching_email_is_case_insensitive() {
            // Allowlist is pre-normalized to lowercase at config load.
            // Incoming email from Google may have any case.
            let list = vec!["alice@example.com".to_string()];
            assert!(
                check_email_allowlist("google", Some("Alice@Example.com"), Some(true), &list)
                    .is_ok()
            );
        }

        #[test]
        fn non_matching_email_is_rejected() {
            let list = vec!["alice@example.com".to_string()];
            assert!(
                check_email_allowlist("google", Some("eve@example.com"), Some(true), &list)
                    .is_err()
            );
        }

        #[test]
        fn unverified_email_is_rejected_even_when_in_allowlist() {
            let list = vec!["alice@example.com".to_string()];
            assert!(
                check_email_allowlist("google", Some("alice@example.com"), Some(false), &list)
                    .is_err()
            );
        }

        #[test]
        fn missing_email_verified_claim_is_rejected_when_allowlist_is_set() {
            let list = vec!["alice@example.com".to_string()];
            assert!(
                check_email_allowlist("google", Some("alice@example.com"), None, &list).is_err()
            );
        }

        #[test]
        fn none_email_is_rejected_when_allowlist_is_set() {
            let list = vec!["alice@example.com".to_string()];
            assert!(check_email_allowlist("google", None, Some(true), &list).is_err());
        }
    }

    #[tokio::test]
    async fn browser_login_renders_a_picker_when_more_than_one_provider_is_configured() {
        let mut config = test_auth_config();
        config.github.client_id = "gh-client".to_string();
        config.github.client_secret = "gh-secret".to_string();
        // GitHubConfig::default()'s scopes is Vec::default() ("[]"), not
        // default_github_scopes() (only applied via #[serde(default = ...)]
        // during deserialization) — validate() now requires `user:email`.
        config.github.scopes = vec!["read:user".to_string(), "user:email".to_string()];
        let state = test_auth_state_with_config(config).await;
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/auth/login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("Sign in with Google"));
        assert!(body.contains("Sign in with GitHub"));
        assert!(body.contains("provider=google"));
        assert!(body.contains("provider=github"));
    }

    #[tokio::test]
    async fn browser_login_skips_the_picker_and_redirects_directly_when_provider_is_given() {
        let mut config = test_auth_config();
        config.github.client_id = "gh-client".to_string();
        config.github.client_secret = "gh-secret".to_string();
        // GitHubConfig::default()'s scopes is Vec::default() ("[]"), not
        // default_github_scopes() (only applied via #[serde(default = ...)]
        // during deserialization) — validate() now requires `user:email`.
        config.github.scopes = vec!["read:user".to_string(), "user:email".to_string()];
        let state = test_auth_state_with_config(config).await;
        let app = router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/auth/login?provider=github")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FOUND);
        let location = response
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(location.contains("github.com/login/oauth/authorize"));
    }

    #[tokio::test]
    async fn authorize_rejects_an_unknown_provider_query_param() {
        let app = router(test_auth_state_with_registered_client().await);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/authorize?response_type=code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=abc&scope=lab&code_challenge=pkce&code_challenge_method=S256&provider=okta")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn authorize_scopes_force_consent_per_provider_not_globally() {
        // Regression test for the engineering-review finding: force_consent
        // must be scoped to the provider about to be used, not "has ANY
        // provider ever issued a refresh token." Seed a Google refresh
        // token, then confirm a fresh GitHub authorize() request still
        // forces consent even though a (Google) refresh token already
        // exists in the DB.
        let mut config = test_auth_config();
        config.github.client_id = "gh-client".to_string();
        config.github.client_secret = "gh-secret".to_string();
        // GitHubConfig::default()'s scopes is Vec::default() ("[]"), not
        // default_github_scopes() (only applied via #[serde(default = ...)]
        // during deserialization) — validate() now requires `user:email`.
        config.github.scopes = vec!["read:user".to_string(), "user:email".to_string()];
        let state = test_auth_state_with_config(config).await;
        state
            .store
            .register_client(RegisteredClient {
                client_id: "client".to_string(),
                redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
                created_at: now_unix(),
            })
            .await
            .unwrap();
        state
            .store
            .upsert_refresh_token(crate::types::RefreshTokenRow {
                refresh_token: "existing-google-refresh".to_string(),
                client_id: "client".to_string(),
                subject: "google-user".to_string(),
                resource: "https://lab.example.com/mcp".to_string(),
                scope: "lab".to_string(),
                provider: "google".to_string(),
                provider_refresh_token: None,
                created_at: now_unix(),
                expires_at: now_unix() + 3600,
            })
            .await
            .unwrap();
        let app = router(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/authorize?response_type=code&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&state=abc&scope=lab&code_challenge=pkce&code_challenge_method=S256&provider=github")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FOUND);
        let location = response
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(location.contains("github.com"));
        assert!(
            location.contains("prompt=consent"),
            "GitHub's first-ever authorize call must still force consent even though Google \
             already has a refresh token on file: {location}"
        );
    }

    /// End-to-end round-trip proving Task 8's manually-renumbered SQL
    /// actually routes to the CORRECT provider's `exchange_code`, not just
    /// that the Rust side compiles. Wires two distinct mock upstream
    /// servers (one per provider) into the same `AuthState` and asserts
    /// only the provider named by `?provider=` at `/auth/login` time
    /// receives the token-exchange call.
    #[tokio::test]
    async fn browser_login_round_trip_calls_only_the_selected_providers_mock_server() {
        use crate::github::GitHubProvider;

        let google_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "google-access-token",
                "refresh_token": "google-refresh-token",
                "expires_in": 3600,
                "id_token": signed_test_id_token(),
            })))
            .mount(&google_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/certs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(test_jwks()))
            .mount(&google_server)
            .await;

        let github_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login/oauth/access_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "github-access-token",
                "scope": "read:user,user:email",
                "token_type": "bearer",
            })))
            .mount(&github_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": 555, "login": "octocat", "email": "user@example.com",
            })))
            .mount(&github_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/user/emails"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"email": "user@example.com", "primary": true, "verified": true},
            ])))
            .mount(&github_server)
            .await;

        let mut config = test_auth_config();
        config.github.client_id = "gh-client".to_string();
        config.github.client_secret = "gh-secret".to_string();
        // GitHubConfig::default()'s scopes is Vec::default() ("[]"), not
        // default_github_scopes() (only applied via #[serde(default = ...)]
        // during deserialization) — validate() now requires `user:email`.
        config.github.scopes = vec!["read:user".to_string(), "user:email".to_string()];
        let base_state = test_auth_state_with_config(config).await;

        let google = GoogleProvider::new(
            "client-id".to_string(),
            "client-secret".to_string(),
            Url::parse("https://lab.example.com/auth/google/callback").unwrap(),
        )
        .unwrap()
        .with_endpoints(
            google_server.uri().parse::<Url>().unwrap(),
            google_server
                .uri()
                .parse::<Url>()
                .unwrap()
                .join("/token")
                .unwrap(),
        )
        .with_jwks_endpoint(
            google_server
                .uri()
                .parse::<Url>()
                .unwrap()
                .join("/certs")
                .unwrap(),
        );
        let github = GitHubProvider::new(
            "gh-client".to_string(),
            "gh-secret".to_string(),
            Url::parse("https://lab.example.com/auth/github/callback").unwrap(),
        )
        .unwrap()
        .with_endpoints(
            github_server
                .uri()
                .parse::<Url>()
                .unwrap()
                .join("login/oauth/authorize")
                .unwrap(),
            github_server
                .uri()
                .parse::<Url>()
                .unwrap()
                .join("login/oauth/access_token")
                .unwrap(),
            github_server
                .uri()
                .parse::<Url>()
                .unwrap()
                .join("user")
                .unwrap(),
            github_server
                .uri()
                .parse::<Url>()
                .unwrap()
                .join("user/emails")
                .unwrap(),
        );
        let mut providers: std::collections::BTreeMap<
            String,
            std::sync::Arc<dyn crate::oauth_provider::OAuthProvider>,
        > = std::collections::BTreeMap::new();
        providers.insert("google".to_string(), std::sync::Arc::new(google));
        providers.insert("github".to_string(), std::sync::Arc::new(github));
        let state = AuthState::for_tests(
            (*base_state.config).clone(),
            base_state.store.clone(),
            (*base_state.signing_keys).clone(),
            providers,
        );
        let app = router(state);

        // Start a browser login explicitly for GitHub.
        let login_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/auth/login?provider=github")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login_response.status(), StatusCode::FOUND);
        let location = Url::parse(
            login_response
                .headers()
                .get(header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap(),
        )
        .unwrap();
        let upstream_state = location
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.into_owned())
            .unwrap();

        // Complete the callback with a fake upstream code.
        let callback_response = app
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "/auth/github/callback?state={upstream_state}&code=upstream-code"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(callback_response.status(), StatusCode::SEE_OTHER);

        // Proves routing AND correct namespacing: the persisted session
        // subject must be `"github:<id>"`, not a bare `"<id>"` that could
        // collide with a Google subject sharing the same DB.
        let set_cookie = callback_response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .expect("callback must set a browser session cookie")
            .to_string();
        let session_id = set_cookie
            .split(';')
            .next()
            .and_then(|kv| kv.split_once('='))
            .map(|(_, value)| value.to_string())
            .expect("session cookie must have a `name=value` pair");
        let session = base_state
            .store
            .find_browser_session(&session_id)
            .await
            .unwrap()
            .expect("browser session must be persisted for the issued cookie");
        assert!(
            session.subject.starts_with("github:"),
            "expected a github-namespaced subject, got `{}`",
            session.subject
        );

        let google_requests = google_server.received_requests().await.unwrap();
        let github_requests = github_server.received_requests().await.unwrap();
        assert!(
            google_requests.is_empty(),
            "selecting provider=github must never call Google's mock server: {google_requests:?}"
        );
        assert!(
            github_requests
                .iter()
                .any(|r| r.url.path() == "/login/oauth/access_token"),
            "expected GitHub's token endpoint to be called: {github_requests:?}"
        );
    }
}
