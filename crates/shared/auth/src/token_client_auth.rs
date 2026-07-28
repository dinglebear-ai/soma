use axum::http::{HeaderMap, header};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use jsonwebtoken::jwk::JwkSet;
use subtle::ConstantTimeEq;

use crate::error::AuthError;
use crate::state::AuthState;
use crate::types::TokenRequest;

mod assertion;

pub(super) const CLIENT_ASSERTION_TYPE: &str =
    "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";

/// RFC 7523 section 2.1 JWT authorization-grant type.
pub(super) const JWT_BEARER_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MachineGrant {
    pub client_id: String,
    pub subject: String,
    pub resource: String,
    pub scope: String,
}

/// Read the `sub` claim of a client assertion *without* verifying it, so the
/// token endpoint can pick which configured client's keys to check the
/// assertion against when `client_id` was not sent as a body parameter.
///
/// The value is a routing hint only: [`authenticate_oauth_client`] still
/// verifies the assertion against that client's JWKS and re-checks that
/// `iss` and `sub` both equal the resolved `client_id`, so a forged `sub`
/// only selects a client whose key then fails to verify.
fn extract_assertion_client_id(assertion: Option<&str>) -> Option<String> {
    assertion::extract_client_id(assertion)
}

/// Treat blank credential parameters as absent.
///
/// Some OAuth clients emit `client_secret=` (or an empty assertion) for public
/// clients rather than omitting the field. Without this, such a request would
/// be read as "this client presented a secret" and rejected even though
/// `token_endpoint_auth_method = "none"` is exactly right for it.
///
/// This only ever removes credentials, so it cannot admit a request that would
/// otherwise be denied - an empty secret matches no configured secret, and an
/// operator who managed to configure an empty `client_secret` now fails closed
/// instead of accepting an empty one. `client_id` is deliberately untouched so
/// the Basic-plus-body ambiguity check in
/// [`apply_basic_client_credentials`] - which runs first - stays strict.
fn discard_blank_credentials(request: &mut TokenRequest) {
    let blank = |value: &Option<String>| value.as_deref().is_some_and(str::is_empty);
    if blank(&request.client_secret) {
        request.client_secret = None;
    }
    if blank(&request.client_assertion) {
        request.client_assertion = None;
    }
    if blank(&request.client_assertion_type) {
        request.client_assertion_type = None;
    }
    if blank(&request.assertion) {
        request.assertion = None;
    }
}

/// Normalize inbound client credentials so every grant sees one shape.
///
/// Folds RFC 6749 section 2.3.1 HTTP Basic credentials into the body
/// parameters, moves a JWT-bearer `assertion` into the client-assertion slot,
/// and recovers `client_id` from the assertion's subject when the client did
/// not send one. Ambiguous credentials (Basic *and* body parameters, or two
/// disagreeing assertions) are rejected here rather than silently resolved.
///
/// Never logs, formats, or returns any part of a secret or assertion.
///
/// The call order is load-bearing: [`apply_basic_client_credentials`] must run
/// first so the Basic-plus-body ambiguity check sees the untouched body
/// parameters, and [`discard_blank_credentials`] must run after it so blanking
/// an empty field can never relax that check. Keeping the sequence in this
/// module means the invariant and the helpers it orders cannot drift apart.
pub(super) fn normalize_client_credentials(
    headers: &HeaderMap,
    request: &mut TokenRequest,
) -> Result<(), AuthError> {
    apply_basic_client_credentials(headers, request)?;
    discard_blank_credentials(request);
    if request.grant_type == JWT_BEARER_GRANT_TYPE {
        adopt_jwt_bearer_assertion(request)?;
    }
    if request.client_id.is_none() {
        request.client_id = extract_assertion_client_id(request.client_assertion.as_deref());
    }
    Ok(())
}

/// Fold the JWT-bearer grant's `assertion` (RFC 7523 section 2.1) into the
/// `client_assertion` slot (RFC 7523 section 2.2).
///
/// soma-auth only mints machine tokens from this grant, and a machine client
/// authenticates with a JWT signed by a key in its configured JWKS - so the
/// same JWT is simultaneously the authorization grant and the client
/// credential. Moving it across means [`machine_grant`] actually verifies it
/// (signature, audience, issuer/subject, expiry, and one-shot `jti`) instead
/// of a security-relevant parameter being silently ignored, which would make
/// this grant a bare alias for `client_credentials`.
///
/// Never guesses: a missing assertion is `invalid_request`, and an `assertion`
/// that disagrees with a separately supplied `client_assertion` - or a
/// `client_assertion_type` naming a different scheme - is `invalid_client`.
fn adopt_jwt_bearer_assertion(request: &mut TokenRequest) -> Result<(), AuthError> {
    let Some(assertion) = request.assertion.take() else {
        return Err(AuthError::Validation(
            "missing `assertion` parameter".to_string(),
        ));
    };
    if request
        .client_assertion
        .as_deref()
        .is_some_and(|supplied| supplied != assertion)
    {
        return Err(invalid_client());
    }
    if request
        .client_assertion_type
        .as_deref()
        .is_some_and(|supplied| supplied != CLIENT_ASSERTION_TYPE)
    {
        return Err(invalid_client());
    }
    request.client_assertion = Some(assertion);
    request.client_assertion_type = Some(CLIENT_ASSERTION_TYPE.to_string());
    Ok(())
}

fn apply_basic_client_credentials(
    headers: &HeaderMap,
    request: &mut TokenRequest,
) -> Result<(), AuthError> {
    let Some((client_id, client_secret)) = basic_client_credentials(headers)? else {
        return Ok(());
    };
    if request.client_id.is_some()
        || request.client_secret.is_some()
        || request.client_assertion.is_some()
        || request.assertion.is_some()
    {
        return Err(invalid_client());
    }
    request.client_id = Some(client_id);
    request.client_secret = Some(client_secret);
    Ok(())
}

pub(super) async fn authenticate_oauth_client(
    state: &AuthState,
    client_id: &str,
    client_secret: Option<&str>,
    client_assertion_type: Option<&str>,
    client_assertion: Option<&str>,
) -> Result<(), AuthError> {
    if let Some(client) = state
        .config
        .machine_clients
        .iter()
        .find(|client| client.client_id == client_id)
    {
        return authenticate_machine_client(
            state,
            client,
            client_secret,
            client_assertion_type,
            client_assertion,
        )
        .await;
    }

    let client = crate::registration::resolve_client(state, client_id)
        .await?
        .ok_or_else(invalid_client)?;
    match client.token_endpoint_auth_method.as_str() {
        "none" if client_secret.is_none() && client_assertion.is_none() => Ok(()),
        "private_key_jwt"
            if client_secret.is_none() && client_assertion_type == Some(CLIENT_ASSERTION_TYPE) =>
        {
            let jwks: JwkSet = serde_json::from_value(client.jwks.ok_or_else(invalid_client)?)
                .map_err(|_| invalid_client())?;
            assertion::validate(
                state,
                client_assertion.ok_or_else(invalid_client)?,
                client_id,
                &jwks,
            )
            .await
        }
        _ => Err(invalid_client()),
    }
}

pub(super) async fn machine_grant(
    state: &AuthState,
    request: &TokenRequest,
) -> Result<MachineGrant, AuthError> {
    let client_id = request
        .client_id
        .as_deref()
        .ok_or_else(|| AuthError::Validation("missing `client_id` parameter".to_string()))?;
    let client = state
        .config
        .machine_clients
        .iter()
        .find(|client| client.client_id == client_id)
        .ok_or_else(invalid_client)?;
    authenticate_machine_client(
        state,
        client,
        request.client_secret.as_deref(),
        request.client_assertion_type.as_deref(),
        request.client_assertion.as_deref(),
    )
    .await?;

    let resource = crate::authorize::validate_resource(state, request.resource.as_deref())
        .map_err(|error| match error {
            AuthError::Validation(message) => AuthError::InvalidScope(message),
            other => other,
        })?;
    if !client
        .resources
        .iter()
        .any(|allowed| allowed.trim_end_matches('/') == resource)
    {
        return Err(AuthError::InvalidScope(
            "requested resource exceeds machine client grant".to_string(),
        ));
    }

    let requested_scope = request.scope.as_deref().unwrap_or_else(|| {
        if client.scopes.is_empty() {
            state.config.default_scope.as_str()
        } else {
            ""
        }
    });
    let requested_scope = if requested_scope.is_empty() {
        client.scopes.join(" ")
    } else {
        requested_scope.to_string()
    };
    let scope =
        crate::authorize::validate_scope(state, &resource, &requested_scope).map_err(|error| {
            match error {
                AuthError::Validation(message) => AuthError::InvalidScope(message),
                other => other,
            }
        })?;
    if !client.scopes.is_empty()
        && !scope
            .split_whitespace()
            .all(|requested| client.scopes.iter().any(|allowed| allowed == requested))
    {
        return Err(AuthError::InvalidScope(
            "requested scope exceeds machine client grant".to_string(),
        ));
    }

    Ok(MachineGrant {
        client_id: client_id.to_string(),
        subject: client_id.to_string(),
        resource,
        scope,
    })
}

async fn authenticate_machine_client(
    state: &AuthState,
    client: &crate::config::MachineClientConfig,
    client_secret: Option<&str>,
    client_assertion_type: Option<&str>,
    client_assertion: Option<&str>,
) -> Result<(), AuthError> {
    match (client_secret, client_assertion) {
        (Some(supplied_secret), None) => {
            let expected_secret = client.client_secret.as_deref().ok_or_else(invalid_client)?;
            if !bool::from(supplied_secret.as_bytes().ct_eq(expected_secret.as_bytes())) {
                return Err(invalid_client());
            }
        }
        (None, Some(assertion)) => {
            if client_assertion_type != Some(CLIENT_ASSERTION_TYPE) {
                return Err(invalid_client());
            }
            let jwks: JwkSet = serde_json::from_value(
                client.jwks.clone().ok_or_else(invalid_client)?,
            )
            .map_err(|error| AuthError::Config(format!("invalid machine client JWKS: {error}")))?;
            assertion::validate(state, assertion, &client.client_id, &jwks).await?;
        }
        _ => return Err(invalid_client()),
    }
    Ok(())
}

fn basic_client_credentials(headers: &HeaderMap) -> Result<Option<(String, String)>, AuthError> {
    let Some(raw) = headers.get(header::AUTHORIZATION) else {
        return Ok(None);
    };
    let raw = raw.to_str().map_err(|_| invalid_client())?;
    let Some((scheme, encoded)) = raw.split_once(' ') else {
        return Err(invalid_client());
    };
    if !scheme.eq_ignore_ascii_case("basic") {
        return Err(invalid_client());
    }
    let decoded = STANDARD
        .decode(encoded.trim())
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .ok_or_else(invalid_client)?;
    let (client_id, client_secret) = decoded.split_once(':').ok_or_else(invalid_client)?;
    let decode_component = |value: &str| {
        url::form_urlencoded::parse(format!("value={value}").as_bytes())
            .next()
            .map(|(_, value)| value.into_owned())
            .ok_or_else(invalid_client)
    };
    Ok(Some((
        decode_component(client_id)?,
        decode_component(client_secret)?,
    )))
}

pub(super) fn invalid_client() -> AuthError {
    AuthError::AuthFailed("invalid client credentials".to_string())
}

#[cfg(test)]
#[path = "token_client_auth_tests.rs"]
mod tests;
