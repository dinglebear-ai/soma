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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MachineGrant {
    pub client_id: String,
    pub subject: String,
    pub resource: String,
    pub scope: String,
}

pub(super) fn extract_assertion_client_id(assertion: Option<&str>) -> Option<String> {
    assertion::extract_client_id(assertion)
}

pub(super) fn apply_basic_client_credentials(
    headers: &HeaderMap,
    request: &mut TokenRequest,
) -> Result<(), AuthError> {
    let Some((client_id, client_secret)) = basic_client_credentials(headers)? else {
        return Ok(());
    };
    if request.client_id.is_some()
        || request.client_secret.is_some()
        || request.client_assertion.is_some()
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
