use jsonwebtoken::jwk::{Jwk, JwkSet, KeyAlgorithm};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::{Deserialize, Serialize};

use crate::error::AuthError;
use crate::state::AuthState;
use crate::util::now_unix;

use super::invalid_client;

#[derive(Debug, Serialize, Deserialize)]
struct ClientAssertionClaims {
    iss: String,
    sub: String,
    aud: String,
    exp: i64,
    iat: i64,
    jti: String,
}

pub(super) fn extract_client_id(assertion: Option<&str>) -> Option<String> {
    assertion
        .and_then(|token| {
            jsonwebtoken::dangerous::insecure_decode::<ClientAssertionClaims>(token).ok()
        })
        .map(|data| data.claims.sub)
}

pub(super) async fn validate(
    state: &AuthState,
    assertion: &str,
    client_id: &str,
    jwks: &JwkSet,
) -> Result<(), AuthError> {
    let header = decode_header(assertion)
        .map_err(|_| AuthError::AuthFailed("invalid client assertion".to_string()))?;
    ensure_allowed_algorithm(header.alg)?;
    let kid = header
        .kid
        .as_deref()
        .ok_or_else(|| AuthError::AuthFailed("client assertion is missing kid".to_string()))?;
    let jwk = jwks
        .find(kid)
        .ok_or_else(|| AuthError::AuthFailed("unknown client assertion key".to_string()))?;
    ensure_jwk_algorithm(jwk, header.alg)?;
    let key = DecodingKey::from_jwk(jwk)
        .map_err(|_| AuthError::AuthFailed("invalid client assertion key".to_string()))?;
    let audience = format!("{}/token", crate::metadata::public_base_url(state));
    let mut validation = Validation::new(header.alg);
    validation.set_audience(&[audience.as_str()]);
    validation.set_issuer(&[client_id]);
    validation.set_required_spec_claims(&["exp", "iat", "iss", "sub", "aud", "jti"]);
    let claims = decode::<ClientAssertionClaims>(assertion, &key, &validation)
        .map_err(|_| AuthError::AuthFailed("invalid client assertion".to_string()))?
        .claims;
    let now = now_unix();
    if claims.iss != client_id
        || claims.sub != client_id
        || claims.aud != audience
        || claims.iat > now + 60
        || claims.exp <= now
    {
        return Err(invalid_client());
    }
    if !state
        .consume_assertion_jti(&claims.iss, &claims.jti, claims.iat, claims.exp)
        .await?
    {
        return Err(AuthError::AuthFailed(
            "invalid or replayed client assertion".to_string(),
        ));
    }
    Ok(())
}

fn ensure_allowed_algorithm(algorithm: Algorithm) -> Result<(), AuthError> {
    if matches!(
        algorithm,
        Algorithm::EdDSA | Algorithm::RS256 | Algorithm::ES256
    ) {
        Ok(())
    } else {
        Err(invalid_client())
    }
}

fn ensure_jwk_algorithm(jwk: &Jwk, algorithm: Algorithm) -> Result<(), AuthError> {
    let matches = match jwk.common.key_algorithm {
        None => true,
        Some(KeyAlgorithm::EdDSA) => algorithm == Algorithm::EdDSA,
        Some(KeyAlgorithm::RS256) => algorithm == Algorithm::RS256,
        Some(KeyAlgorithm::ES256) => algorithm == Algorithm::ES256,
        _ => false,
    };
    if matches {
        Ok(())
    } else {
        Err(invalid_client())
    }
}
