use std::fmt;
use std::time::Duration;

use reqwest::{Client, Method};
use serde_json::{json, Value};

use crate::error::{GotifyError, Result};
use crate::{http, GotifyConfig};

/// HTTP REST client for Gotify push-notification servers.
///
/// Authentication uses the `X-Gotify-Key` header with one of two distinct
/// token kinds: a **client token** for management operations (messages,
/// applications, clients, current user) and an **app token** for sending
/// messages. `health`/`version` need no token at all.
///
/// Builds and holds one pooled [`reqwest::Client`] for its lifetime — clone
/// and share a `GotifyClient` rather than constructing a new one per
/// request, so requests reuse connections instead of paying a fresh TLS
/// handshake each time.
///
/// `Debug` redacts both tokens, same as [`GotifyConfig`].
#[derive(Clone)]
pub struct GotifyClient {
    http: Client,
    url: String,
    client_token: String,
    app_token: String,
    request_timeout: Duration,
}

impl fmt::Debug for GotifyClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GotifyClient")
            .field("url", &self.url)
            .field(
                "client_token",
                &format_args!("<redacted, {} bytes>", self.client_token.len()),
            )
            .field(
                "app_token",
                &format_args!("<redacted, {} bytes>", self.app_token.len()),
            )
            .field("request_timeout", &self.request_timeout)
            .finish()
    }
}

impl GotifyClient {
    /// Builds a client from `cfg`.
    ///
    /// Unlike `client_token`/`app_token` (each validated lazily, only by the
    /// specific calls that need them), `url` is required upfront — every
    /// call needs it, so failing fast here is strictly more useful than
    /// deferring to the first request.
    ///
    /// # Errors
    /// Returns [`GotifyError::MissingUrl`] if `cfg.url` is empty, or
    /// [`GotifyError::ClientBuild`] if the underlying HTTP client fails to
    /// construct.
    pub fn new(cfg: &GotifyConfig) -> Result<Self> {
        if cfg.url.is_empty() {
            return Err(GotifyError::MissingUrl);
        }
        let http = http::build_client(cfg)?;
        Ok(Self {
            http,
            url: cfg.url.trim_end_matches('/').to_string(),
            client_token: cfg.client_token.clone(),
            app_token: cfg.app_token.clone(),
            request_timeout: cfg.request_timeout,
        })
    }

    /// Reconstructs the [`GotifyConfig`] this client was built from.
    pub fn config(&self) -> GotifyConfig {
        GotifyConfig {
            url: self.url.clone(),
            client_token: self.client_token.clone(),
            app_token: self.app_token.clone(),
            request_timeout: self.request_timeout,
        }
    }

    fn require_client_token(&self) -> Result<&str> {
        if self.client_token.is_empty() {
            return Err(GotifyError::MissingClientToken);
        }
        Ok(&self.client_token)
    }

    fn require_app_token(&self) -> Result<&str> {
        if self.app_token.is_empty() {
            return Err(GotifyError::MissingAppToken);
        }
        Ok(&self.app_token)
    }

    #[allow(clippy::too_many_arguments)]
    async fn request(
        &self,
        token: Option<&str>,
        action: &str,
        method: Method,
        path: &str,
        query: Option<&[(&str, String)]>,
        body: Option<&Value>,
    ) -> Result<Value> {
        http::request_json(
            &self.http, &self.url, token, action, method, path, query, body,
        )
        .await
    }

    // ── unauthenticated ───────────────────────────────────────────────────────

    /// Server health check. Needs no token.
    ///
    /// # Errors
    /// See [`GotifyError`] for the failure cases this can return.
    pub async fn health(&self) -> Result<Value> {
        self.request(None, "health", Method::GET, "health", None, None)
            .await
    }

    /// Server version. Needs no token.
    ///
    /// # Errors
    /// See [`GotifyError`] for the failure cases this can return.
    pub async fn version(&self) -> Result<Value> {
        self.request(None, "version", Method::GET, "version", None, None)
            .await
    }

    // ── current user ─────────────────────────────────────────────────────────

    /// Authenticated user info. Requires a client token.
    ///
    /// # Errors
    /// Returns [`GotifyError::MissingClientToken`] if no client token is
    /// configured; see [`GotifyError`] for the other failure cases this can
    /// return.
    pub async fn me(&self) -> Result<Value> {
        let token = self.require_client_token()?;
        self.request(Some(token), "me", Method::GET, "current/user", None, None)
            .await
    }

    // ── messages ──────────────────────────────────────────────────────────────

    /// Lists messages, optionally scoped to one application. Requires a
    /// client token.
    ///
    /// `limit` defaults to 50 server-side if `None`.
    ///
    /// # Errors
    /// Returns [`GotifyError::MissingClientToken`] if no client token is
    /// configured; see [`GotifyError`] for the other failure cases this can
    /// return.
    pub async fn messages(
        &self,
        app_id: Option<i64>,
        limit: Option<i64>,
        since: Option<i64>,
    ) -> Result<Value> {
        let token = self.require_client_token()?;
        let path = match app_id {
            Some(id) => format!("application/{id}/message"),
            None => "message".to_string(),
        };
        let mut query: Vec<(&str, String)> = vec![("limit", limit.unwrap_or(50).to_string())];
        if let Some(since) = since {
            query.push(("since", since.to_string()));
        }
        self.request(
            Some(token),
            "messages",
            Method::GET,
            &path,
            Some(&query),
            None,
        )
        .await
    }

    /// Sends a notification. Requires an app token (not a client token).
    ///
    /// # Errors
    /// Returns [`GotifyError::MissingAppToken`] if no app token is
    /// configured; see [`GotifyError`] for the other failure cases this can
    /// return.
    pub async fn send_message(
        &self,
        message: &str,
        title: Option<&str>,
        priority: Option<i64>,
        extras: Option<Value>,
    ) -> Result<Value> {
        let token = self.require_app_token()?;
        let mut body = json!({ "message": message });
        if let Some(title) = title {
            body["title"] = json!(title);
        }
        if let Some(priority) = priority {
            body["priority"] = json!(priority);
        }
        if let Some(extras) = extras {
            body["extras"] = extras;
        }
        self.request(
            Some(token),
            "send_message",
            Method::POST,
            "message",
            None,
            Some(&body),
        )
        .await
    }

    /// Deletes one message. Requires a client token.
    ///
    /// # Errors
    /// Returns [`GotifyError::MissingClientToken`] if no client token is
    /// configured; see [`GotifyError`] for the other failure cases this can
    /// return.
    pub async fn delete_message(&self, id: i64) -> Result<Value> {
        let token = self.require_client_token()?;
        self.request(
            Some(token),
            "delete_message",
            Method::DELETE,
            &format!("message/{id}"),
            None,
            None,
        )
        .await
    }

    /// Deletes every message. Requires a client token.
    ///
    /// # Errors
    /// Returns [`GotifyError::MissingClientToken`] if no client token is
    /// configured; see [`GotifyError`] for the other failure cases this can
    /// return.
    pub async fn delete_all_messages(&self) -> Result<Value> {
        let token = self.require_client_token()?;
        self.request(
            Some(token),
            "delete_all_messages",
            Method::DELETE,
            "message",
            None,
            None,
        )
        .await
    }

    // ── applications ──────────────────────────────────────────────────────────

    /// Lists applications. Requires a client token.
    ///
    /// # Errors
    /// Returns [`GotifyError::MissingClientToken`] if no client token is
    /// configured; see [`GotifyError`] for the other failure cases this can
    /// return.
    pub async fn applications(&self) -> Result<Value> {
        let token = self.require_client_token()?;
        self.request(
            Some(token),
            "applications",
            Method::GET,
            "application",
            None,
            None,
        )
        .await
    }

    /// Creates an application. Requires a client token.
    ///
    /// # Errors
    /// Returns [`GotifyError::MissingClientToken`] if no client token is
    /// configured; see [`GotifyError`] for the other failure cases this can
    /// return.
    pub async fn create_application(
        &self,
        name: &str,
        description: Option<&str>,
        default_priority: Option<i64>,
    ) -> Result<Value> {
        let token = self.require_client_token()?;
        let mut body = json!({ "name": name });
        if let Some(description) = description {
            body["description"] = json!(description);
        }
        if let Some(default_priority) = default_priority {
            body["defaultPriority"] = json!(default_priority);
        }
        self.request(
            Some(token),
            "create_application",
            Method::POST,
            "application",
            None,
            Some(&body),
        )
        .await
    }

    /// Updates an application. Requires a client token.
    ///
    /// # Errors
    /// Returns [`GotifyError::MissingClientToken`] if no client token is
    /// configured; see [`GotifyError`] for the other failure cases this can
    /// return.
    pub async fn update_application(
        &self,
        app_id: i64,
        name: Option<&str>,
        description: Option<&str>,
        default_priority: Option<i64>,
    ) -> Result<Value> {
        let token = self.require_client_token()?;
        let mut body = json!({});
        if let Some(name) = name {
            body["name"] = json!(name);
        }
        if let Some(description) = description {
            body["description"] = json!(description);
        }
        if let Some(default_priority) = default_priority {
            body["defaultPriority"] = json!(default_priority);
        }
        self.request(
            Some(token),
            "update_application",
            Method::PUT,
            &format!("application/{app_id}"),
            None,
            Some(&body),
        )
        .await
    }

    /// Deletes an application. Requires a client token.
    ///
    /// # Errors
    /// Returns [`GotifyError::MissingClientToken`] if no client token is
    /// configured; see [`GotifyError`] for the other failure cases this can
    /// return.
    pub async fn delete_application(&self, app_id: i64) -> Result<Value> {
        let token = self.require_client_token()?;
        self.request(
            Some(token),
            "delete_application",
            Method::DELETE,
            &format!("application/{app_id}"),
            None,
            None,
        )
        .await
    }

    // ── clients ───────────────────────────────────────────────────────────────

    /// Lists clients. Requires a client token.
    ///
    /// # Errors
    /// Returns [`GotifyError::MissingClientToken`] if no client token is
    /// configured; see [`GotifyError`] for the other failure cases this can
    /// return.
    pub async fn clients(&self) -> Result<Value> {
        let token = self.require_client_token()?;
        self.request(Some(token), "clients", Method::GET, "client", None, None)
            .await
    }

    /// Creates a client. Requires a client token.
    ///
    /// # Errors
    /// Returns [`GotifyError::MissingClientToken`] if no client token is
    /// configured; see [`GotifyError`] for the other failure cases this can
    /// return.
    pub async fn create_client(&self, name: &str) -> Result<Value> {
        let token = self.require_client_token()?;
        self.request(
            Some(token),
            "create_client",
            Method::POST,
            "client",
            None,
            Some(&json!({ "name": name })),
        )
        .await
    }

    /// Deletes a client. Requires a client token.
    ///
    /// # Errors
    /// Returns [`GotifyError::MissingClientToken`] if no client token is
    /// configured; see [`GotifyError`] for the other failure cases this can
    /// return.
    pub async fn delete_client(&self, client_id: i64) -> Result<Value> {
        let token = self.require_client_token()?;
        self.request(
            Some(token),
            "delete_client",
            Method::DELETE,
            &format!("client/{client_id}"),
            None,
            None,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(url: &str) -> GotifyConfig {
        GotifyConfig {
            url: url.to_string(),
            ..GotifyConfig::default()
        }
    }

    #[test]
    fn new_rejects_a_missing_url() {
        let err = GotifyClient::new(&GotifyConfig::default()).unwrap_err();

        assert!(matches!(err, GotifyError::MissingUrl));
    }

    #[test]
    fn new_succeeds_without_any_token_configured() {
        // Unlike unifi's single api_key, Gotify's two token kinds are each
        // validated lazily by only the calls that need them — a client with
        // no tokens at all is legitimate (health/version still work).
        GotifyClient::new(&config("https://gotify.local")).unwrap();
    }

    #[test]
    fn config_round_trips_a_non_default_request_timeout() {
        let cfg = GotifyConfig {
            url: "https://gotify.local".to_string(),
            request_timeout: Duration::from_secs(90),
            ..GotifyConfig::default()
        };

        let client = GotifyClient::new(&cfg).unwrap();

        assert_eq!(client.config().request_timeout, Duration::from_secs(90));
    }
}
