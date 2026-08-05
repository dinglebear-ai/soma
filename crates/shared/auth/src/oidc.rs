use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::{Algorithm, DecodingKey, Header, Validation, decode, decode_header};
use reqwest::Url;
use reqwest::header;
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::error::AuthError;
use crate::oauth_provider::ProviderExchange;
use crate::provider_http::{RequestErrors, RequestTrace, read_json_response};
use crate::util::fingerprint;

/// Which RFC 6749 §2.3.1 client-authentication method to use on the token
/// endpoint (`exchange_code`/`refresh`).
///
/// Google's token endpoint accepts `client_secret` in the POST body
/// regardless of how the client credential was provisioned, so Google keeps
/// using [`Self::ClientSecretPost`] (this crate's original, pre-multi-provider
/// behavior). Authelia's OIDC provider defaults confidential clients to
/// `client_secret_basic` unless the operator explicitly sets
/// `token_endpoint_auth_method: client_secret_post` on the client — sending
/// `client_secret` in the body instead of the `Authorization` header would
/// silently fail authentication against that (documented) default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TokenAuthMethod {
    /// `client_id`/`client_secret` in the POST body (RFC 6749 §2.3.1,
    /// "NOT RECOMMENDED" but widely supported; Google's default).
    ClientSecretPost,
    /// `client_id`/`client_secret` via HTTP Basic `Authorization` header
    /// (RFC 6749 §2.3.1's default/recommended method; Authelia's default).
    ClientSecretBasic,
}

const DEFAULT_JWKS_TTL: Duration = Duration::from_secs(60 * 60);
/// Per-request timeout on the JWKS GET. Bound aggressively (5s) so a slow
/// upstream JWKS endpoint cannot starve a tokio worker holding the JWKS
/// write lock. Token exchange / refresh keep the provider's own looser
/// timeout because those can legitimately take longer.
const JWKS_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Deserialize)]
pub(crate) struct IdTokenClaims {
    pub iss: String,
    pub sub: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub email_verified: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct OidcTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    id_token: String,
}

#[derive(Clone, Debug, Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Clone, Debug, Deserialize)]
struct Jwk {
    kid: String,
    #[serde(default)]
    alg: Option<String>,
    n: String,
    e: String,
}

#[derive(Clone, Debug)]
struct CachedJwks {
    jwks: Jwks,
    expires_at: Instant,
}

/// Shared RS256 ID-token verifier for OIDC-shaped upstream providers
/// (Google, Authelia). Caches the provider's JWKS document and validates
/// signature, expiry, audience, and issuer on every [`Self::verify`] call.
///
/// `Clone` (all fields are cheap-to-clone handles: `Arc`, `String`,
/// `reqwest::Client`, `Url`) so `GoogleProvider`/`AutheliaProvider` can stay
/// `#[derive(Clone)]` themselves, matching their pre-existing public API.
#[derive(Clone)]
pub(crate) struct OidcVerifier {
    provider_id: &'static str,
    issuer: String,
    /// Second accepted issuer value, checked alongside `issuer` on every
    /// [`Self::verify`] call. Google's real ID tokens carry either the
    /// `https://` form or the bare `accounts.google.com` form depending on
    /// token version — pre-extraction `google.rs` accepted both
    /// (`claims.iss != GOOGLE_ISSUER && claims.iss != "accounts.google.com"`).
    /// `None` for providers with a single canonical issuer (Authelia).
    alt_issuer: Option<String>,
    jwks_endpoint: Url,
    http: reqwest::Client,
    jwks_cache: Arc<RwLock<Option<CachedJwks>>>,
    /// Token-endpoint client authentication method. See [`TokenAuthMethod`].
    token_auth_method: TokenAuthMethod,
}

impl OidcVerifier {
    pub(crate) fn new(
        provider_id: &'static str,
        issuer: String,
        jwks_endpoint: Url,
        http: reqwest::Client,
    ) -> Self {
        Self {
            provider_id,
            issuer,
            alt_issuer: None,
            jwks_endpoint,
            http,
            jwks_cache: Arc::new(RwLock::new(None)),
            token_auth_method: TokenAuthMethod::ClientSecretPost,
        }
    }

    /// Accept a second issuer value alongside `issuer` in [`Self::verify`].
    /// See [`Self::alt_issuer`]'s doc comment for why this exists.
    #[must_use]
    pub(crate) fn with_alt_issuer(mut self, alt_issuer: impl Into<String>) -> Self {
        self.alt_issuer = Some(alt_issuer.into());
        self
    }

    /// Override the token-endpoint client authentication method. Defaults to
    /// [`TokenAuthMethod::ClientSecretPost`] (Google's behavior); Authelia
    /// opts into [`TokenAuthMethod::ClientSecretBasic`].
    #[must_use]
    pub(crate) fn with_token_auth_method(mut self, method: TokenAuthMethod) -> Self {
        self.token_auth_method = method;
        self
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_jwks_endpoint(mut self, jwks_endpoint: Url) -> Self {
        self.jwks_endpoint = jwks_endpoint;
        self
    }

    /// Builds the token-endpoint POST request, applying `client_id`/
    /// `client_secret` per [`Self::token_auth_method`]: appended to the form
    /// body for [`TokenAuthMethod::ClientSecretPost`], or sent via the HTTP
    /// `Authorization: Basic` header (and omitted from the body, per RFC
    /// 6749 §3.2.1 — a client authenticated at the transport level does not
    /// also repeat its credentials in the body) for
    /// [`TokenAuthMethod::ClientSecretBasic`].
    fn token_request<'a>(
        &self,
        http: &reqwest::Client,
        token_endpoint: &Url,
        client_id: &'a str,
        client_secret: &'a str,
        mut form: Vec<(&'a str, &'a str)>,
    ) -> reqwest::RequestBuilder {
        let mut request = http.post(token_endpoint.clone());
        match self.token_auth_method {
            TokenAuthMethod::ClientSecretPost => {
                form.push(("client_id", client_id));
                form.push(("client_secret", client_secret));
            }
            TokenAuthMethod::ClientSecretBasic => {
                request = request.basic_auth(client_id, Some(client_secret));
            }
        }
        request.form(&form)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn exchange_code(
        &self,
        http: &reqwest::Client,
        token_endpoint: &Url,
        client_id: &str,
        client_secret: &str,
        redirect_uri: &Url,
        code: &str,
        code_verifier: &str,
    ) -> Result<ProviderExchange, AuthError> {
        let trace = RequestTrace::start(self.provider_id, "code_exchange", "POST", token_endpoint);
        info!(
            provider = self.provider_id,
            oauth_code_id = %fingerprint(code),
            redirect_uri_id = %fingerprint(redirect_uri.as_str()),
            "oauth upstream code exchange started"
        );
        let payload: OidcTokenResponse = read_json_response(
            trace,
            self.token_request(
                http,
                token_endpoint,
                client_id,
                client_secret,
                vec![
                    ("grant_type", "authorization_code"),
                    ("code", code),
                    ("redirect_uri", redirect_uri.as_str()),
                    ("code_verifier", code_verifier),
                ],
            ),
            RequestErrors::new(
                self.provider_id,
                format!("exchange {} auth code", self.provider_id),
                format!("{} token endpoint error", self.provider_id),
                format!("decode {} token response", self.provider_id),
            ),
        )
        .await?;
        self.finish_exchange(payload, client_id, "code_exchange")
            .await
    }

    pub(crate) async fn refresh(
        &self,
        http: &reqwest::Client,
        token_endpoint: &Url,
        client_id: &str,
        client_secret: &str,
        refresh_token: &str,
    ) -> Result<ProviderExchange, AuthError> {
        let trace = RequestTrace::start(self.provider_id, "refresh", "POST", token_endpoint);
        info!(
            provider = self.provider_id,
            refresh_token_id = %fingerprint(refresh_token),
            "oauth upstream refresh started"
        );
        let payload: OidcTokenResponse = read_json_response(
            trace,
            self.token_request(
                http,
                token_endpoint,
                client_id,
                client_secret,
                vec![
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh_token),
                ],
            ),
            RequestErrors::new(
                self.provider_id,
                format!("refresh {} token", self.provider_id),
                format!("{} refresh endpoint error", self.provider_id),
                format!("decode {} refresh response", self.provider_id),
            ),
        )
        .await?;
        self.finish_exchange(payload, client_id, "refresh").await
    }

    async fn finish_exchange(
        &self,
        payload: OidcTokenResponse,
        client_id: &str,
        operation: &'static str,
    ) -> Result<ProviderExchange, AuthError> {
        let claims = self.verify(&payload.id_token, client_id).await?;
        if operation == "code_exchange" {
            info!(
                provider = self.provider_id,
                subject_id = %fingerprint(&claims.sub),
                has_refresh_token = payload.refresh_token.is_some(),
                expires_in_secs = payload.expires_in,
                "oauth upstream code exchange succeeded"
            );
        } else {
            info!(
                provider = self.provider_id,
                subject_id = %fingerprint(&claims.sub),
                has_refresh_token = payload.refresh_token.is_some(),
                expires_in_secs = payload.expires_in,
                "oauth upstream refresh succeeded"
            );
        }
        Ok(ProviderExchange {
            subject: claims.sub,
            email: claims.email,
            email_verified: claims.email_verified,
            access_token: payload.access_token,
            refresh_token: payload.refresh_token,
            expires_in: payload.expires_in,
            id_token: Some(payload.id_token),
        })
    }

    pub(crate) async fn verify(
        &self,
        id_token: &str,
        audience: &str,
    ) -> Result<IdTokenClaims, AuthError> {
        let header = decode_header(id_token).map_err(|error| {
            AuthError::Storage(format!("verify {} id_token: {error}", self.provider_id))
        })?;
        validate_header_alg(self.provider_id, &header)?;
        let kid = header.kid.ok_or_else(|| {
            AuthError::Storage(format!("{} id_token is missing a key id", self.provider_id))
        })?;
        let key = self.find_jwk_for_kid(&kid).await?;
        if let Some(alg) = key.alg.as_deref()
            && alg != "RS256"
        {
            return Err(AuthError::Storage(format!(
                "{} JWKS key `{}` uses unsupported algorithm `{alg}`",
                self.provider_id, key.kid
            )));
        }

        let decoding_key = DecodingKey::from_rsa_components(&key.n, &key.e).map_err(|error| {
            AuthError::Storage(format!(
                "build {} id_token decoding key: {error}",
                self.provider_id
            ))
        })?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.validate_exp = true;
        validation.leeway = 0;
        validation.set_audience(&[audience]);

        let claims = decode::<IdTokenClaims>(id_token, &decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(|error| {
                AuthError::Storage(format!("invalid {} id_token: {error}", self.provider_id))
            })?;

        let issuer_matches = claims.iss == self.issuer
            || self
                .alt_issuer
                .as_deref()
                .is_some_and(|alt| claims.iss == alt);
        if !issuer_matches {
            return Err(AuthError::Storage(format!(
                "invalid {} id_token issuer `{}`",
                self.provider_id, claims.iss
            )));
        }

        Ok(claims)
    }

    async fn find_jwk_for_kid(&self, kid: &str) -> Result<Jwk, AuthError> {
        let jwks = self.fetch_jwks().await?;
        if let Some(key) = jwks.keys.into_iter().find(|key| key.kid == kid) {
            return Ok(key);
        }

        debug!(
            provider = self.provider_id,
            kid, "jwks cache miss for token key id; refreshing"
        );
        self.refresh_jwks()
            .await?
            .keys
            .into_iter()
            .find(|key| key.kid == kid)
            .ok_or_else(|| {
                AuthError::Storage(format!(
                    "{} id_token key id was not found in JWKS",
                    self.provider_id
                ))
            })
    }

    async fn fetch_jwks(&self) -> Result<Jwks, AuthError> {
        if let Some(jwks) = self.cached_jwks().await {
            debug!(provider = self.provider_id, "jwks cache hit");
            return Ok(jwks);
        }

        let jwks = {
            let mut cache = self.jwks_cache.write().await;
            if let Some(cached) = cache
                .as_ref()
                .filter(|cached| cached.expires_at > Instant::now())
            {
                debug!(
                    provider = self.provider_id,
                    "jwks cache hit after refresh lock"
                );
                cached.jwks.clone()
            } else {
                self.refresh_jwks_locked(&mut cache).await?
            }
        };
        Ok(jwks)
    }

    async fn refresh_jwks(&self) -> Result<Jwks, AuthError> {
        let mut cache = self.jwks_cache.write().await;
        self.refresh_jwks_locked(&mut cache).await
    }

    async fn refresh_jwks_locked(&self, cache: &mut Option<CachedJwks>) -> Result<Jwks, AuthError> {
        let trace = RequestTrace::start(self.provider_id, "fetch_jwks", "GET", &self.jwks_endpoint);
        let response = self
            .http
            .get(self.jwks_endpoint.clone())
            .timeout(JWKS_FETCH_TIMEOUT)
            .send()
            .await
            .map_err(|error| {
                trace.error(None, &error);
                warn!(provider = self.provider_id, error = %error, "jwks request failed");
                AuthError::Storage(format!("fetch {} jwks: {error}", self.provider_id))
            })?;
        let status = response.status();
        let ttl = jwks_ttl(response.headers());
        let response = response.error_for_status().map_err(|error| {
            trace.error(Some(status), &error);
            warn!(provider = self.provider_id, error = %error, "jwks request returned error status");
            AuthError::Storage(format!("{} jwks endpoint error: {error}", self.provider_id))
        })?;
        trace.finish(status);
        let jwks = response.json::<Jwks>().await.map_err(|error| {
            warn!(provider = self.provider_id, error = %error, "jwks payload unreadable");
            AuthError::Storage(format!(
                "decode {} jwks response: {error}",
                self.provider_id
            ))
        })?;

        *cache = Some(CachedJwks {
            jwks: jwks.clone(),
            expires_at: Instant::now() + ttl,
        });

        Ok(jwks)
    }

    async fn cached_jwks(&self) -> Option<Jwks> {
        let cache = self.jwks_cache.read().await;
        cache
            .as_ref()
            .filter(|cached| cached.expires_at > Instant::now())
            .map(|cached| cached.jwks.clone())
    }
}

fn jwks_ttl(headers: &header::HeaderMap) -> Duration {
    headers
        .get(header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_max_age)
        .map_or(DEFAULT_JWKS_TTL, Duration::from_secs)
}

fn parse_max_age(cache_control: &str) -> Option<u64> {
    cache_control.split(',').find_map(|directive| {
        let directive = directive.trim();
        let value = directive.strip_prefix("max-age=")?;
        value.parse::<u64>().ok()
    })
}

fn validate_header_alg(provider_id: &str, header: &Header) -> Result<(), AuthError> {
    if header.alg != Algorithm::RS256 {
        return Err(AuthError::Storage(format!(
            "verify {provider_id} id_token: unsupported algorithm `{:?}`",
            header.alg
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_max_age_reads_cache_control_max_age() {
        assert_eq!(super::parse_max_age("public, max-age=3600"), Some(3600));
        assert_eq!(super::parse_max_age("no-cache"), None);
    }
}
