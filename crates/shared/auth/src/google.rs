use std::time::Duration;

use async_trait::async_trait;
use reqwest::Url;
use tracing::debug;

use crate::error::AuthError;
use crate::oauth_provider::{AuthorizeUrlRequest, OAuthProvider, ProviderExchange};
use crate::oidc::OidcVerifier;
use crate::provider_http::build_authorize_url;
use crate::util::fingerprint;

const GOOGLE_AUTHORIZE_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_JWKS_ENDPOINT: &str = "https://www.googleapis.com/oauth2/v3/certs";
const GOOGLE_ISSUER: &str = "https://accounts.google.com";
/// Google ID tokens can also carry this bare-form issuer (no scheme) —
/// accepted alongside [`GOOGLE_ISSUER`] to preserve pre-extraction behavior
/// (`google.rs`'s `verify_id_token` used to check both forms directly).
const GOOGLE_ISSUER_ALT: &str = "accounts.google.com";
const GOOGLE_HTTP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct GoogleProvider {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: Url,
    pub scopes: Vec<String>,
    pub http: reqwest::Client,
    authorize_endpoint: Url,
    token_endpoint: Url,
    verifier: OidcVerifier,
}

impl std::fmt::Debug for GoogleProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoogleProvider")
            .field("client_id", &self.client_id)
            .field("redirect_uri", &self.redirect_uri)
            .field("scopes", &self.scopes)
            .finish_non_exhaustive()
    }
}

impl GoogleProvider {
    pub fn new(
        client_id: String,
        client_secret: String,
        redirect_uri: Url,
    ) -> Result<Self, AuthError> {
        crate::provider_http::install_rustls_default_once();
        let http = reqwest::Client::builder()
            .timeout(GOOGLE_HTTP_TIMEOUT)
            .build()
            .map_err(|error| {
                AuthError::Storage(format!("build google oauth http client: {error}"))
            })?;
        let authorize_endpoint = Url::parse(GOOGLE_AUTHORIZE_ENDPOINT).map_err(|error| {
            AuthError::Config(format!("parse google authorize endpoint: {error}"))
        })?;
        let token_endpoint = Url::parse(GOOGLE_TOKEN_ENDPOINT)
            .map_err(|error| AuthError::Config(format!("parse google token endpoint: {error}")))?;
        let jwks_endpoint = Url::parse(GOOGLE_JWKS_ENDPOINT)
            .map_err(|error| AuthError::Config(format!("parse google jwks endpoint: {error}")))?;
        let verifier = OidcVerifier::new(
            "google",
            GOOGLE_ISSUER.to_string(),
            jwks_endpoint,
            http.clone(),
        )
        .with_alt_issuer(GOOGLE_ISSUER_ALT);

        Ok(Self {
            client_id,
            client_secret,
            redirect_uri,
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ],
            http,
            authorize_endpoint,
            token_endpoint,
            verifier,
        })
    }

    #[cfg(test)]
    #[must_use]
    pub fn with_endpoints(mut self, authorize_endpoint: Url, token_endpoint: Url) -> Self {
        self.authorize_endpoint = authorize_endpoint;
        self.token_endpoint = token_endpoint;
        self
    }

    #[cfg(test)]
    #[must_use]
    pub fn with_jwks_endpoint(mut self, jwks_endpoint: Url) -> Self {
        self.verifier = self.verifier.with_jwks_endpoint(jwks_endpoint);
        self
    }
}

#[async_trait]
impl OAuthProvider for GoogleProvider {
    fn provider_id(&self) -> &'static str {
        "google"
    }

    fn callback_path(&self) -> &str {
        self.redirect_uri.path()
    }

    fn authorize_url(&self, request: &AuthorizeUrlRequest) -> Result<Url, AuthError> {
        let scope = self.scopes.join(" ");
        let url = build_authorize_url(
            &self.authorize_endpoint,
            &self.client_id,
            &self.redirect_uri,
            &self.scopes,
            request,
            &[
                ("access_type", "offline"),
                ("include_granted_scopes", "true"),
            ],
        );
        debug!(
            provider = "google",
            oauth_state_id = %fingerprint(&request.state),
            scope = %scope,
            redirect_uri = %self.redirect_uri,
            "oauth upstream authorize URL constructed"
        );
        Ok(url)
    }

    async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<ProviderExchange, AuthError> {
        self.verifier
            .exchange_code(
                &self.http,
                &self.token_endpoint,
                &self.client_id,
                &self.client_secret,
                &self.redirect_uri,
                code,
                code_verifier,
            )
            .await
    }

    async fn refresh(&self, refresh_token: &str) -> Result<ProviderExchange, AuthError> {
        self.verifier
            .refresh(
                &self.http,
                &self.token_endpoint,
                &self.client_id,
                &self.client_secret,
                refresh_token,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use rsa::RsaPrivateKey;
    use rsa::pkcs8::EncodePrivateKey;
    use rsa::rand_core::{TryCryptoRng, TryRng, UnwrapErr};
    use rsa::traits::PublicKeyParts;
    use serde_json::json;
    use std::sync::OnceLock;
    use url::Url;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{AuthorizeUrlRequest, GoogleProvider};
    use crate::oauth_provider::OAuthProvider;

    #[test]
    fn google_authorize_url_includes_offline_access_prompt_and_pkce() {
        let provider = test_google_provider();
        let request = sample_request();
        let url = provider.authorize_url(&request).unwrap();
        assert!(url.as_str().contains("access_type=offline"));
        assert!(url.as_str().contains("prompt=consent"));
        assert!(url.as_str().contains("code_challenge="));
    }

    #[test]
    fn google_authorize_url_omits_prompt_when_consent_not_forced() {
        let provider = test_google_provider();
        let mut request = sample_request();
        request.force_consent = false;
        let url = provider.authorize_url(&request).unwrap();
        assert!(url.as_str().contains("access_type=offline"));
        assert!(!url.as_str().contains("prompt="));
    }

    #[tokio::test]
    async fn google_exchange_parses_subject_and_refresh_token() {
        let provider = mocked_google_provider().await;
        let token = provider.exchange_code("code", "verifier").await.unwrap();
        assert_eq!(token.subject, "google-subject-123");
        assert_eq!(token.refresh_token.as_deref(), Some("refresh-token"));
    }

    #[tokio::test]
    async fn google_exchange_rejects_unsigned_id_tokens() {
        let provider = mocked_google_provider_with_id_token(test_id_token()).await;
        let error = provider
            .exchange_code("code", "verifier")
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("verify google id_token"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn google_exchange_rejects_wrong_audience_in_id_token() {
        let provider =
            mocked_google_provider_with_id_token(signed_test_id_token("other-client", false, true))
                .await;
        let error = provider
            .exchange_code("code", "verifier")
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("invalid google id_token"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn google_exchange_rejects_expired_id_token() {
        let provider =
            mocked_google_provider_with_id_token(signed_test_id_token("client-id", true, true))
                .await;
        let error = provider
            .exchange_code("code", "verifier")
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("invalid google id_token"),
            "unexpected error: {error}"
        );
    }

    #[tokio::test]
    async fn google_exchange_rejects_wrong_issuer_in_id_token() {
        let provider =
            mocked_google_provider_with_id_token(signed_test_id_token("client-id", false, false))
                .await;
        let error = provider
            .exchange_code("code", "verifier")
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("issuer"),
            "unexpected error: {error}"
        );
    }

    /// Regression test for the alt-issuer fix: pre-extraction `google.rs`
    /// accepted ID tokens carrying either the `https://accounts.google.com`
    /// form OR the bare `accounts.google.com` form. Prove the bare form is
    /// still accepted after the `OidcVerifier` extraction, not just that it
    /// isn't rejected.
    #[tokio::test]
    async fn google_exchange_accepts_bare_form_issuer_in_id_token() {
        let provider = mocked_google_provider_with_id_token(signed_test_id_token_with_issuer(
            "client-id",
            "accounts.google.com",
        ))
        .await;
        let exchange = provider.exchange_code("code", "verifier").await;
        assert!(
            exchange.is_ok(),
            "bare-form issuer must be accepted: {exchange:?}"
        );
        assert_eq!(exchange.unwrap().subject, "google-subject-123");
    }

    #[tokio::test]
    async fn google_exchange_reuses_cached_jwks() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "google-access-token",
                "refresh_token": "refresh-token",
                "expires_in": 3600,
                "id_token": signed_test_id_token("client-id", false, true),
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/certs"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("Cache-Control", "public, max-age=3600")
                    .set_body_json(test_jwks()),
            )
            .mount(&server)
            .await;

        let provider = test_google_provider()
            .with_endpoints(
                server.uri().parse::<Url>().unwrap(),
                server.uri().parse::<Url>().unwrap().join("/token").unwrap(),
            )
            .with_jwks_endpoint(server.uri().parse::<Url>().unwrap().join("/certs").unwrap());

        provider.exchange_code("code-1", "verifier").await.unwrap();
        provider.exchange_code("code-2", "verifier").await.unwrap();

        let requests = server.received_requests().await.unwrap();
        let jwks_requests = requests
            .iter()
            .filter(|request| request.url.path() == "/certs")
            .count();
        assert_eq!(jwks_requests, 1);
    }

    #[tokio::test]
    async fn google_exchange_succeeds_on_first_jwks_fetch_with_no_pre_seeded_cache() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "google-access-token",
                "refresh_token": "refresh-token",
                "expires_in": 3600,
                "id_token": signed_test_id_token("client-id", false, true),
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/certs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(test_jwks()))
            .mount(&server)
            .await;

        let provider = test_google_provider()
            .with_endpoints(
                server.uri().parse::<Url>().unwrap(),
                server.uri().parse::<Url>().unwrap().join("/token").unwrap(),
            )
            .with_jwks_endpoint(server.uri().parse::<Url>().unwrap().join("/certs").unwrap());

        let exchange = provider.exchange_code("code", "verifier").await.unwrap();
        assert_eq!(exchange.subject, "google-subject-123");

        let requests = server.received_requests().await.unwrap();
        let jwks_requests = requests
            .iter()
            .filter(|request| request.url.path() == "/certs")
            .count();
        assert_eq!(jwks_requests, 1);
    }

    fn test_google_provider() -> GoogleProvider {
        GoogleProvider::new(
            "client-id".to_string(),
            "client-secret".to_string(),
            Url::parse("https://app.example.com/auth/google/callback").unwrap(),
        )
        .unwrap()
    }

    async fn mocked_google_provider() -> MockedGoogleProvider {
        mocked_google_provider_with_id_token(signed_test_id_token("client-id", false, true)).await
    }

    struct MockedGoogleProvider {
        provider: GoogleProvider,
        _server: MockServer,
    }

    impl std::ops::Deref for MockedGoogleProvider {
        type Target = GoogleProvider;

        fn deref(&self) -> &Self::Target {
            &self.provider
        }
    }

    async fn mocked_google_provider_with_id_token(id_token: String) -> MockedGoogleProvider {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "google-access-token",
                "refresh_token": "refresh-token",
                "expires_in": 3600,
                "id_token": id_token,
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/certs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(test_jwks()))
            .mount(&server)
            .await;

        let provider = test_google_provider()
            .with_endpoints(
                server.uri().parse::<Url>().unwrap(),
                server.uri().parse::<Url>().unwrap().join("/token").unwrap(),
            )
            .with_jwks_endpoint(server.uri().parse::<Url>().unwrap().join("/certs").unwrap());

        MockedGoogleProvider {
            provider,
            _server: server,
        }
    }

    fn sample_request() -> AuthorizeUrlRequest {
        AuthorizeUrlRequest {
            state: "state-123".to_string(),
            code_challenge: "challenge".to_string(),
            code_challenge_method: "S256".to_string(),
            force_consent: true,
        }
    }

    fn test_id_token() -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(br#"{"sub":"google-subject-123"}"#);
        format!("{header}.{payload}.")
    }

    fn signed_test_id_token(client_id: &str, expired: bool, valid_issuer: bool) -> String {
        let issuer = if valid_issuer {
            "https://accounts.google.com"
        } else {
            "https://evil.example.com"
        };
        signed_test_id_token_with_issuer_and_expiry(client_id, issuer, expired)
    }

    fn signed_test_id_token_with_issuer(client_id: &str, issuer: &str) -> String {
        signed_test_id_token_with_issuer_and_expiry(client_id, issuer, false)
    }

    fn signed_test_id_token_with_issuer_and_expiry(
        client_id: &str,
        issuer: &str,
        expired: bool,
    ) -> String {
        let claims = json!({
            "iss": issuer,
            "aud": client_id,
            "sub": "google-subject-123",
            "email": "user@example.com",
            "iat": (unix_now() - 10) as usize,
            "exp": if expired { (unix_now() - 3600) as usize } else { (unix_now() + 3600) as usize },
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

    fn test_encoding_key() -> EncodingKey {
        let pem = test_rsa_key().to_pkcs8_pem(Default::default()).unwrap();
        EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap()
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
