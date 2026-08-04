use std::time::Duration;

use async_trait::async_trait;
use reqwest::Url;
use serde::Deserialize;
use tracing::info;

use crate::error::AuthError;
use crate::oauth_provider::{AuthorizeUrlRequest, OAuthProvider, ProviderExchange};
use crate::provider_http::{RequestErrors, RequestTrace, build_authorize_url, read_json_response};
use crate::util::fingerprint;

const GITHUB_AUTHORIZE_ENDPOINT: &str = "https://github.com/login/oauth/authorize";
const GITHUB_TOKEN_ENDPOINT: &str = "https://github.com/login/oauth/access_token";
const GITHUB_USER_ENDPOINT: &str = "https://api.github.com/user";
const GITHUB_USER_EMAILS_ENDPOINT: &str = "https://api.github.com/user/emails";
const GITHUB_HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const GITHUB_USER_AGENT: &str = "soma-auth";

#[derive(Clone)]
pub struct GitHubProvider {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: Url,
    pub scopes: Vec<String>,
    pub http: reqwest::Client,
    authorize_endpoint: Url,
    token_endpoint: Url,
    user_endpoint: Url,
    user_emails_endpoint: Url,
}

impl std::fmt::Debug for GitHubProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitHubProvider")
            .field("client_id", &self.client_id)
            .field("redirect_uri", &self.redirect_uri)
            .field("scopes", &self.scopes)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Deserialize)]
struct GitHubTokenResponse {
    access_token: String,
}

/// GitHub's token endpoint documented behavior: on an invalid, expired, or
/// already-used authorization code it returns **HTTP 200** with an error
/// body (`{"error":"bad_verification_code",...}`) instead of a non-2xx
/// status — so `provider_http::read_json_response`'s `error_for_status()`
/// never triggers, and we must distinguish success from failure by shape
/// after the fact via this untagged enum, rather than by status code.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum GitHubTokenResult {
    Success(GitHubTokenResponse),
    Error(GitHubTokenErrorResponse),
}

#[derive(Debug, Deserialize)]
struct GitHubTokenErrorResponse {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubUser {
    id: u64,
    #[serde(default)]
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubUserEmail {
    email: String,
    primary: bool,
    verified: bool,
}

impl GitHubProvider {
    pub fn new(
        client_id: String,
        client_secret: String,
        redirect_uri: Url,
    ) -> Result<Self, AuthError> {
        crate::provider_http::install_rustls_default_once();
        let http = reqwest::Client::builder()
            .timeout(GITHUB_HTTP_TIMEOUT)
            .user_agent(GITHUB_USER_AGENT)
            .build()
            .map_err(|error| {
                AuthError::Storage(format!("build github oauth http client: {error}"))
            })?;
        let authorize_endpoint = Url::parse(GITHUB_AUTHORIZE_ENDPOINT).map_err(|error| {
            AuthError::Config(format!("parse github authorize endpoint: {error}"))
        })?;
        let token_endpoint = Url::parse(GITHUB_TOKEN_ENDPOINT)
            .map_err(|error| AuthError::Config(format!("parse github token endpoint: {error}")))?;
        let user_endpoint = Url::parse(GITHUB_USER_ENDPOINT)
            .map_err(|error| AuthError::Config(format!("parse github user endpoint: {error}")))?;
        let user_emails_endpoint = Url::parse(GITHUB_USER_EMAILS_ENDPOINT).map_err(|error| {
            AuthError::Config(format!("parse github user emails endpoint: {error}"))
        })?;
        Ok(Self {
            client_id,
            client_secret,
            redirect_uri,
            scopes: vec!["read:user".to_string(), "user:email".to_string()],
            http,
            authorize_endpoint,
            token_endpoint,
            user_endpoint,
            user_emails_endpoint,
        })
    }

    #[cfg(test)]
    #[must_use]
    pub fn with_endpoints(
        mut self,
        authorize_endpoint: Url,
        token_endpoint: Url,
        user_endpoint: Url,
        user_emails_endpoint: Url,
    ) -> Self {
        self.authorize_endpoint = authorize_endpoint;
        self.token_endpoint = token_endpoint;
        self.user_endpoint = user_endpoint;
        self.user_emails_endpoint = user_emails_endpoint;
        self
    }

    /// Fetches `GET /user` and `GET /user/emails` **concurrently** via
    /// `tokio::try_join!` — they are independent, both authenticated with the
    /// same bearer token, and running them sequentially (as an earlier draft
    /// of this plan did) needlessly widens the worst-case timeout envelope: 3
    /// sequential hops each independently subject to `GITHUB_HTTP_TIMEOUT`
    /// (30s) can chain up to ~90s before failing, vs Google/Authelia's ~35s
    /// worst case (30s token exchange + 5s JWKS). Joining the two GETs caps
    /// GitHub's worst case at ~60s (30s token exchange + max(30s, 30s)).
    async fn fetch_exchange(
        &self,
        payload: GitHubTokenResponse,
    ) -> Result<ProviderExchange, AuthError> {
        let (user, verified_email) = tokio::try_join!(
            self.fetch_user(&payload.access_token),
            self.fetch_primary_verified_email(&payload.access_token),
        )?;

        let (email, email_verified) = match verified_email {
            Some(verified) => (Some(verified), Some(true)),
            None => (user.email, None),
        };

        info!(
            provider = "github",
            subject_id = %fingerprint(&user.id.to_string()),
            "oauth upstream code exchange succeeded"
        );

        // GitHubProvider::exchange_code must never set refresh_token — GitHub
        // OAuth Apps don't issue one. The real, always-on guard against a
        // `refresh_tokens` row naming `provider = "github"` reaching
        // `GitHubProvider::refresh` (which unconditionally errors) lives in
        // `token::refresh_token_grant`, right before it calls
        // `provider.refresh(...)` — not here, and not as a `debug_assert!`
        // (which compiles out of release builds and, being right next to
        // this hardcoded `None` literal, could only ever catch someone
        // editing both lines inconsistently in the same change).
        Ok(ProviderExchange {
            subject: user.id.to_string(),
            email,
            email_verified,
            access_token: payload.access_token,
            refresh_token: None,
            expires_in: None,
            id_token: None,
        })
    }

    async fn fetch_user(&self, access_token: &str) -> Result<GitHubUser, AuthError> {
        let trace = RequestTrace::start("github", "fetch_user", "GET", &self.user_endpoint);
        read_json_response(
            trace,
            self.http
                .get(self.user_endpoint.clone())
                .bearer_auth(access_token)
                .header(reqwest::header::ACCEPT, "application/vnd.github+json"),
            RequestErrors::new(
                "github",
                "fetch github user",
                "github user endpoint error",
                "decode github user response",
            ),
        )
        .await
    }

    async fn fetch_primary_verified_email(
        &self,
        access_token: &str,
    ) -> Result<Option<String>, AuthError> {
        let trace = RequestTrace::start(
            "github",
            "fetch_user_emails",
            "GET",
            &self.user_emails_endpoint,
        );
        let emails: Vec<GitHubUserEmail> = read_json_response(
            trace,
            self.http
                .get(self.user_emails_endpoint.clone())
                .bearer_auth(access_token)
                .header(reqwest::header::ACCEPT, "application/vnd.github+json"),
            RequestErrors::new(
                "github",
                "fetch github user emails",
                "github user emails endpoint error",
                "decode github user emails response",
            ),
        )
        .await?;
        Ok(emails
            .into_iter()
            .find(|entry| entry.primary && entry.verified)
            .map(|entry| entry.email))
    }
}

#[async_trait]
impl OAuthProvider for GitHubProvider {
    fn provider_id(&self) -> &'static str {
        "github"
    }

    fn callback_path(&self) -> &str {
        self.redirect_uri.path()
    }

    fn authorize_url(&self, request: &AuthorizeUrlRequest) -> Result<Url, AuthError> {
        Ok(build_authorize_url(
            &self.authorize_endpoint,
            &self.client_id,
            &self.redirect_uri,
            &self.scopes,
            request,
            &[],
        ))
    }

    async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<ProviderExchange, AuthError> {
        let trace = RequestTrace::start("github", "code_exchange", "POST", &self.token_endpoint);
        info!(
            provider = "github",
            oauth_code_id = %fingerprint(code),
            redirect_uri = %self.redirect_uri,
            "oauth upstream code exchange started"
        );
        let payload: GitHubTokenResult = read_json_response(
            trace,
            self.http
                .post(self.token_endpoint.clone())
                .header(reqwest::header::ACCEPT, "application/json")
                .form(&[
                    ("grant_type", "authorization_code"),
                    ("code", code),
                    ("client_id", self.client_id.as_str()),
                    ("client_secret", self.client_secret.as_str()),
                    ("redirect_uri", self.redirect_uri.as_str()),
                    ("code_verifier", code_verifier),
                ]),
            RequestErrors::new(
                "github",
                "exchange github auth code",
                "github token endpoint error",
                "decode github token response",
            ),
        )
        .await?;
        let payload = match payload {
            GitHubTokenResult::Success(payload) => payload,
            GitHubTokenResult::Error(error) => {
                return Err(AuthError::InvalidGrant(format!(
                    "github token exchange failed: {} ({})",
                    error.error,
                    error
                        .error_description
                        .as_deref()
                        .unwrap_or("no description")
                )));
            }
        };
        self.fetch_exchange(payload).await
    }

    async fn refresh(&self, _refresh_token: &str) -> Result<ProviderExchange, AuthError> {
        Err(AuthError::Config(
            "github oauth apps do not support token refresh — access tokens do not expire; \
             the user must re-authenticate via github once their locally issued refresh \
             token expires"
                .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{AuthorizeUrlRequest, GitHubProvider};
    use crate::error::AuthError;
    use crate::oauth_provider::OAuthProvider;

    #[tokio::test]
    async fn github_exchange_uses_numeric_id_as_subject_and_primary_verified_email() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login/oauth/access_token"))
            .and(header("accept", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "gho_test-token",
                "scope": "read:user,user:email",
                "token_type": "bearer",
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": 9182310,
                "login": "octocat",
                "email": null,
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/user/emails"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"email": "secondary@example.com", "primary": false, "verified": true},
                {"email": "primary@example.com", "primary": true, "verified": true},
            ])))
            .mount(&server)
            .await;

        let base = url::Url::parse(&server.uri()).unwrap();
        let provider = test_github_provider().with_endpoints(
            base.join("login/oauth/authorize").unwrap(),
            base.join("login/oauth/access_token").unwrap(),
            base.join("user").unwrap(),
            base.join("user/emails").unwrap(),
        );

        let exchange = provider.exchange_code("code", "verifier").await.unwrap();
        assert_eq!(exchange.subject, "9182310");
        assert_eq!(exchange.email.as_deref(), Some("primary@example.com"));
        assert_eq!(exchange.email_verified, Some(true));
        assert!(exchange.id_token.is_none());
        assert!(exchange.refresh_token.is_none());
    }

    /// Coverage for the fallback branch in `fetch_exchange`: when
    /// `/user/emails` has no primary+verified entry (here, none of the
    /// returned addresses are verified), `email_verified` must be `None`
    /// and `email` must fall back to `/user`'s inline `email` field.
    #[tokio::test]
    async fn github_exchange_falls_back_to_inline_user_email_when_no_primary_verified_entry() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login/oauth/access_token"))
            .and(header("accept", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "gho_test-token",
                "scope": "read:user,user:email",
                "token_type": "bearer",
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": 9182310,
                "login": "octocat",
                "email": "inline@example.com",
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/user/emails"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                {"email": "unverified@example.com", "primary": true, "verified": false},
                {"email": "secondary@example.com", "primary": false, "verified": true},
            ])))
            .mount(&server)
            .await;

        let base = url::Url::parse(&server.uri()).unwrap();
        let provider = test_github_provider().with_endpoints(
            base.join("login/oauth/authorize").unwrap(),
            base.join("login/oauth/access_token").unwrap(),
            base.join("user").unwrap(),
            base.join("user/emails").unwrap(),
        );

        let exchange = provider.exchange_code("code", "verifier").await.unwrap();
        assert_eq!(exchange.subject, "9182310");
        assert_eq!(exchange.email.as_deref(), Some("inline@example.com"));
        assert_eq!(exchange.email_verified, None);
    }

    /// Regression test: GitHub's token endpoint returns HTTP 200 with an
    /// error body (no non-2xx status) on an invalid/expired/reused
    /// authorization code — `exchange_code` must classify this as
    /// `AuthError::InvalidGrant`, not fall through to `AuthError::Decode`
    /// from a failed `GitHubTokenResponse` deserialization.
    #[tokio::test]
    async fn github_exchange_classifies_200_ok_error_body_as_invalid_grant() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/login/oauth/access_token"))
            .and(header("accept", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "error": "bad_verification_code",
                "error_description": "The code passed is incorrect or expired.",
            })))
            .mount(&server)
            .await;

        let base = url::Url::parse(&server.uri()).unwrap();
        let provider = test_github_provider().with_endpoints(
            base.join("login/oauth/authorize").unwrap(),
            base.join("login/oauth/access_token").unwrap(),
            base.join("user").unwrap(),
            base.join("user/emails").unwrap(),
        );

        let error = provider
            .exchange_code("code", "verifier")
            .await
            .unwrap_err();
        assert!(
            matches!(error, AuthError::InvalidGrant(_)),
            "expected InvalidGrant, got {error:?}"
        );
    }

    #[tokio::test]
    async fn github_refresh_always_errors() {
        let provider = test_github_provider();
        let error = provider.refresh("whatever").await.unwrap_err();
        assert!(error.to_string().contains("do not support token refresh"));
    }

    #[test]
    fn github_authorize_url_uses_read_user_and_user_email_scopes() {
        let provider = test_github_provider();
        let request = AuthorizeUrlRequest {
            state: "state-123".to_string(),
            code_challenge: "challenge".to_string(),
            code_challenge_method: "S256".to_string(),
            force_consent: false,
        };
        let url = provider.authorize_url(&request).unwrap();
        assert!(url.as_str().contains("scope=read%3Auser+user%3Aemail"));
    }

    fn test_github_provider() -> GitHubProvider {
        GitHubProvider::new(
            "client-id".to_string(),
            "client-secret".to_string(),
            url::Url::parse("https://app.example.com/auth/github/callback").unwrap(),
        )
        .unwrap()
    }
}
