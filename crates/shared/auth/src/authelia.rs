use std::time::Duration;

use async_trait::async_trait;
use reqwest::Url;
use serde::Deserialize;
use tracing::info;

use crate::error::AuthError;
use crate::oauth_provider::{AuthorizeUrlRequest, OAuthProvider, ProviderExchange};
use crate::oidc::OidcVerifier;
use crate::provider_http::{RequestErrors, RequestTrace, read_json_response};
use crate::util::fingerprint;

const AUTHELIA_HTTP_TIMEOUT: Duration = Duration::from_secs(30);
/// Authelia's OpenID Connect 1.0 Provider mounts its endpoints at these
/// fixed paths off the issuer — they are not configurable per-deployment.
const AUTHELIA_AUTHORIZE_PATH: &str = "api/oidc/authorization";
const AUTHELIA_TOKEN_PATH: &str = "api/oidc/token";
const AUTHELIA_JWKS_PATH: &str = "api/oidc/jwks";

#[derive(Clone)]
pub struct AutheliaProvider {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: Url,
    pub scopes: Vec<String>,
    pub http: reqwest::Client,
    issuer: Url,
    authorize_endpoint: Url,
    token_endpoint: Url,
    verifier: OidcVerifier,
}

impl std::fmt::Debug for AutheliaProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AutheliaProvider")
            .field("client_id", &self.client_id)
            .field("issuer", &self.issuer)
            .field("redirect_uri", &self.redirect_uri)
            .field("scopes", &self.scopes)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Deserialize)]
struct AutheliaTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    id_token: String,
}

impl AutheliaProvider {
    pub fn new(
        issuer: Url,
        client_id: String,
        client_secret: String,
        redirect_uri: Url,
    ) -> Result<Self, AuthError> {
        drop(rustls::crypto::ring::default_provider().install_default());
        let http = reqwest::Client::builder()
            .timeout(AUTHELIA_HTTP_TIMEOUT)
            .build()
            .map_err(|error| {
                AuthError::Storage(format!("build authelia oauth http client: {error}"))
            })?;
        let authorize_endpoint = issuer.join(AUTHELIA_AUTHORIZE_PATH).map_err(|error| {
            AuthError::Config(format!("build authelia authorize endpoint: {error}"))
        })?;
        let token_endpoint = issuer.join(AUTHELIA_TOKEN_PATH).map_err(|error| {
            AuthError::Config(format!("build authelia token endpoint: {error}"))
        })?;
        let jwks_endpoint = issuer
            .join(AUTHELIA_JWKS_PATH)
            .map_err(|error| AuthError::Config(format!("build authelia jwks endpoint: {error}")))?;
        let verifier = OidcVerifier::new(
            "authelia",
            issuer.as_str().trim_end_matches('/').to_string(),
            jwks_endpoint,
            http.clone(),
        );

        Ok(Self {
            client_id,
            client_secret,
            redirect_uri,
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
                "offline_access".to_string(),
            ],
            http,
            issuer,
            authorize_endpoint,
            token_endpoint,
            verifier,
        })
    }

    #[cfg(test)]
    #[must_use]
    pub fn with_endpoints(
        mut self,
        issuer: Url,
        authorize_endpoint: Url,
        token_endpoint: Url,
        jwks_endpoint: Url,
    ) -> Self {
        self.authorize_endpoint = authorize_endpoint;
        self.token_endpoint = token_endpoint;
        self.verifier = OidcVerifier::new(
            "authelia",
            issuer.as_str().trim_end_matches('/').to_string(),
            jwks_endpoint,
            self.http.clone(),
        );
        self.issuer = issuer;
        self
    }

    pub fn authorize_url(&self, request: &AuthorizeUrlRequest) -> Result<Url, AuthError> {
        let mut url = self.authorize_endpoint.clone();
        let scope = self.scopes.join(" ");
        url.query_pairs_mut()
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", self.redirect_uri.as_str())
            .append_pair("response_type", "code")
            .append_pair("scope", &scope)
            .append_pair("state", &request.state)
            .append_pair("code_challenge", &request.code_challenge)
            .append_pair("code_challenge_method", &request.code_challenge_method);
        if request.force_consent {
            url.query_pairs_mut().append_pair("prompt", "consent");
        }
        Ok(url)
    }

    pub async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<ProviderExchange, AuthError> {
        let trace = RequestTrace::start("authelia", "code_exchange", "POST", &self.token_endpoint);
        info!(
            provider = "authelia",
            oauth_code_id = %fingerprint(code),
            redirect_uri = %self.redirect_uri,
            "oauth upstream code exchange started"
        );
        let payload: AutheliaTokenResponse = read_json_response(
            trace,
            self.http.post(self.token_endpoint.clone()).form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("redirect_uri", self.redirect_uri.as_str()),
                ("code_verifier", code_verifier),
            ]),
            RequestErrors::new(
                "authelia",
                "exchange authelia auth code",
                "authelia token endpoint error",
                "decode authelia token response",
            ),
        )
        .await?;
        let claims = self
            .verifier
            .verify(&payload.id_token, &self.client_id)
            .await?;
        info!(
            provider = "authelia",
            subject_id = %fingerprint(&claims.sub),
            has_refresh_token = payload.refresh_token.is_some(),
            expires_in_secs = payload.expires_in,
            "oauth upstream code exchange succeeded"
        );
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

    pub async fn refresh(&self, refresh_token: &str) -> Result<ProviderExchange, AuthError> {
        let trace = RequestTrace::start("authelia", "refresh", "POST", &self.token_endpoint);
        info!(
            provider = "authelia",
            refresh_token_id = %fingerprint(refresh_token),
            "oauth upstream refresh started"
        );
        let payload: AutheliaTokenResponse = read_json_response(
            trace,
            self.http.post(self.token_endpoint.clone()).form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
            ]),
            RequestErrors::new(
                "authelia",
                "refresh authelia token",
                "authelia refresh endpoint error",
                "decode authelia refresh response",
            ),
        )
        .await?;
        let claims = self
            .verifier
            .verify(&payload.id_token, &self.client_id)
            .await?;
        info!(
            provider = "authelia",
            subject_id = %fingerprint(&claims.sub),
            has_refresh_token = payload.refresh_token.is_some(),
            expires_in_secs = payload.expires_in,
            "oauth upstream refresh succeeded"
        );
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
}

#[async_trait]
impl OAuthProvider for AutheliaProvider {
    fn provider_id(&self) -> &'static str {
        "authelia"
    }

    fn callback_path(&self) -> &str {
        self.redirect_uri.path()
    }

    fn authorize_url(&self, request: &AuthorizeUrlRequest) -> Result<Url, AuthError> {
        Self::authorize_url(self, request)
    }

    async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<ProviderExchange, AuthError> {
        Self::exchange_code(self, code, code_verifier).await
    }

    async fn refresh(&self, refresh_token: &str) -> Result<ProviderExchange, AuthError> {
        Self::refresh(self, refresh_token).await
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use jsonwebtoken::{Algorithm, Header, encode};
    use rsa::RsaPrivateKey;
    use rsa::pkcs8::EncodePrivateKey;
    use rsa::rand_core::{TryCryptoRng, TryRng, UnwrapErr};
    use rsa::traits::PublicKeyParts;
    use serde_json::json;
    use std::sync::OnceLock;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{AutheliaProvider, AuthorizeUrlRequest};

    #[test]
    fn authelia_authorize_url_requests_offline_access_via_scope_not_access_type() {
        let provider = test_authelia_provider();
        let request = AuthorizeUrlRequest {
            state: "state-123".to_string(),
            scope: "lab".to_string(),
            code_challenge: "challenge".to_string(),
            code_challenge_method: "S256".to_string(),
            force_consent: true,
        };
        let url = provider.authorize_url(&request).unwrap();
        assert!(
            url.as_str()
                .contains("scope=openid+email+profile+offline_access")
        );
        assert!(!url.as_str().contains("access_type="));
        assert!(url.as_str().contains("prompt=consent"));
    }

    #[tokio::test]
    async fn authelia_exchange_parses_subject_from_id_token() {
        let server = MockServer::start().await;
        let issuer = Url::parse(&server.uri()).unwrap();
        Mock::given(method("POST"))
            .and(path("/api/oidc/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "authelia-access-token",
                "refresh_token": "authelia-refresh-token",
                "expires_in": 3600,
                "id_token": signed_test_id_token(&issuer, "client-id"),
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/oidc/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(test_jwks()))
            .mount(&server)
            .await;

        let provider = test_authelia_provider().with_endpoints(
            issuer.clone(),
            issuer.join("api/oidc/authorization").unwrap(),
            issuer.join("api/oidc/token").unwrap(),
            issuer.join("api/oidc/jwks").unwrap(),
        );

        let exchange = provider.exchange_code("code", "verifier").await.unwrap();
        assert_eq!(exchange.subject, "authelia-subject-123");
        assert_eq!(
            exchange.refresh_token.as_deref(),
            Some("authelia-refresh-token")
        );
    }

    use reqwest::Url;

    fn test_authelia_provider() -> AutheliaProvider {
        AutheliaProvider::new(
            Url::parse("https://auth.example.com").unwrap(),
            "client-id".to_string(),
            "client-secret".to_string(),
            Url::parse("https://lab.example.com/auth/authelia/callback").unwrap(),
        )
        .unwrap()
    }

    fn signed_test_id_token(issuer: &Url, client_id: &str) -> String {
        let claims = json!({
            "iss": issuer.as_str().trim_end_matches('/'),
            "aud": client_id,
            "sub": "authelia-subject-123",
            "email": "user@example.com",
            "email_verified": true,
            "iat": (unix_now() - 10) as usize,
            "exp": (unix_now() + 3600) as usize,
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
                "n": URL_SAFE_NO_PAD.encode(public_key.n_bytes()),
                "e": URL_SAFE_NO_PAD.encode(public_key.e_bytes()),
            }]
        })
    }

    fn test_rsa_key() -> &'static RsaPrivateKey {
        static TEST_RSA_KEY: OnceLock<RsaPrivateKey> = OnceLock::new();
        TEST_RSA_KEY.get_or_init(|| {
            let mut rng = UnwrapErr(TestRng);
            RsaPrivateKey::new(&mut rng, 2048).unwrap()
        })
    }

    fn test_encoding_key() -> jsonwebtoken::EncodingKey {
        let pem = test_rsa_key().to_pkcs8_pem(Default::default()).unwrap();
        jsonwebtoken::EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap()
    }

    fn unix_now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    struct TestRng;

    impl TryRng for TestRng {
        type Error = getrandom::Error;

        fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
            let mut bytes = [0u8; 4];
            getrandom::fill(&mut bytes)?;
            Ok(u32::from_le_bytes(bytes))
        }

        fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
            let mut bytes = [0u8; 8];
            getrandom::fill(&mut bytes)?;
            Ok(u64::from_le_bytes(bytes))
        }

        fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
            getrandom::fill(dst)
        }
    }

    impl TryCryptoRng for TestRng {}
}
