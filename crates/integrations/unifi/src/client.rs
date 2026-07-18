use std::fmt;
use std::time::Duration;

use reqwest::{Client, Method};
use serde_json::Value;

use crate::error::{Result, UnifiError};
use crate::{http, UnifiConfig};

/// HTTP REST client for UniFi controllers.
///
/// Supports both modern UniFi OS controllers (behind `/proxy/network`) and
/// legacy controllers (no prefix, typically port 8443). Authentication uses
/// the `X-API-KEY` header.
///
/// Builds and holds one pooled [`reqwest::Client`] for its lifetime — clone
/// and share a `UnifiClient` rather than constructing a new one per request,
/// so requests reuse connections instead of paying a fresh TLS handshake
/// each time.
///
/// `Debug` redacts the API key, same as [`UnifiConfig`].
#[derive(Clone)]
pub struct UnifiClient {
    http: Client,
    /// Base URL, e.g. `https://unifi.local`, with any trailing slash trimmed.
    pub url: String,
    api_key: String,
    site: String,
    skip_tls_verify: bool,
    legacy: bool,
    request_timeout: Duration,
}

impl fmt::Debug for UnifiClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnifiClient")
            .field("url", &self.url)
            .field(
                "api_key",
                &format_args!("<redacted, {} bytes>", self.api_key.len()),
            )
            .field("site", &self.site)
            .field("skip_tls_verify", &self.skip_tls_verify)
            .field("legacy", &self.legacy)
            .field("request_timeout", &self.request_timeout)
            .finish()
    }
}

impl UnifiClient {
    /// Builds a client from `cfg`.
    ///
    /// # Errors
    /// Returns [`UnifiError::MissingUrl`] or [`UnifiError::MissingApiKey`] if the
    /// corresponding config field is empty, or [`UnifiError::ClientBuild`] if the
    /// underlying HTTP client fails to construct.
    pub fn new(cfg: &UnifiConfig) -> Result<Self> {
        if cfg.url.is_empty() {
            return Err(UnifiError::MissingUrl);
        }
        if cfg.api_key.is_empty() {
            return Err(UnifiError::MissingApiKey);
        }
        let http = http::build_client(cfg)?;
        Ok(Self {
            http,
            url: cfg.url.trim_end_matches('/').to_string(),
            api_key: cfg.api_key.clone(),
            site: cfg.site.clone(),
            skip_tls_verify: cfg.skip_tls_verify,
            legacy: cfg.legacy,
            request_timeout: cfg.request_timeout,
        })
    }

    /// Site slug this client targets (`UnifiConfig::site`).
    pub fn site(&self) -> &str {
        &self.site
    }

    /// Whether this client targets a legacy controller (no `/proxy/network` prefix).
    pub fn legacy(&self) -> bool {
        self.legacy
    }

    /// Reconstructs the [`UnifiConfig`] this client was built from.
    pub fn config(&self) -> UnifiConfig {
        UnifiConfig {
            url: self.url.clone(),
            api_key: self.api_key.clone(),
            site: self.site.clone(),
            skip_tls_verify: self.skip_tls_verify,
            legacy: self.legacy,
            request_timeout: self.request_timeout,
        }
    }

    /// Issues a request against this client's controller, reusing its pooled
    /// connection. This is the primitive the dynamic action dispatcher
    /// ([`crate::ActionDispatcher`]) builds on; the named methods below
    /// (`clients`, `devices`, ...) are thin, discoverable wrappers around it.
    ///
    /// # Errors
    /// See [`UnifiError`] for the failure cases this can return.
    pub async fn request_json(
        &self,
        method: Method,
        path: &str,
        query: Option<&Value>,
        body: Option<&Value>,
    ) -> Result<Value> {
        http::request_json(
            &self.http,
            &self.url,
            &self.api_key,
            method,
            path,
            query,
            body,
        )
        .await
    }

    fn site_path(&self, suffix: &str) -> String {
        let prefix = if self.legacy { "" } else { "/proxy/network" };
        format!("{prefix}/api/s/{site}/{suffix}", site = self.site)
    }

    fn self_path(&self) -> &'static str {
        if self.legacy {
            "/api/self"
        } else {
            "/proxy/network/api/self"
        }
    }

    async fn get(&self, path: &str, action: &'static str) -> Result<Value> {
        let span = tracing::info_span!("upstream", %action, site = %self.site);
        let _guard = span.enter();
        tracing::debug!(url = %self.url, "calling UniFi API");
        let result = self.request_json(Method::GET, path, None, None).await;
        match &result {
            Ok(v) => {
                let count = v
                    .get("data")
                    .and_then(|d| d.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                tracing::debug!(action, count, "upstream call ok");
            }
            Err(e) => tracing::warn!(action, error = %e, "upstream call failed"),
        }
        result
    }

    /// Connected clients (wireless and wired).
    ///
    /// # Errors
    /// See [`UnifiError`] for the failure cases this can return.
    pub async fn clients(&self) -> Result<Value> {
        self.get(&self.site_path("stat/sta"), "clients").await
    }

    /// Network devices: APs, switches, gateways.
    ///
    /// # Errors
    /// See [`UnifiError`] for the failure cases this can return.
    pub async fn devices(&self) -> Result<Value> {
        self.get(&self.site_path("stat/device"), "devices").await
    }

    /// WLAN (WiFi network) configurations.
    ///
    /// # Errors
    /// See [`UnifiError`] for the failure cases this can return.
    pub async fn wlans(&self) -> Result<Value> {
        self.get(&self.site_path("rest/wlanconf"), "wlans").await
    }

    /// Site health summary.
    ///
    /// # Errors
    /// See [`UnifiError`] for the failure cases this can return.
    pub async fn health(&self) -> Result<Value> {
        self.get(&self.site_path("stat/health"), "health").await
    }

    /// Active alarms / alerts.
    ///
    /// # Errors
    /// See [`UnifiError`] for the failure cases this can return.
    pub async fn alarms(&self) -> Result<Value> {
        self.get(&self.site_path("rest/alarm"), "alarms").await
    }

    /// Recent events.
    ///
    /// # Errors
    /// See [`UnifiError`] for the failure cases this can return.
    pub async fn events(&self) -> Result<Value> {
        self.get(&self.site_path("rest/event"), "events").await
    }

    /// Controller system info.
    ///
    /// # Errors
    /// See [`UnifiError`] for the failure cases this can return.
    pub async fn sysinfo(&self) -> Result<Value> {
        self.get(&self.site_path("stat/sysinfo"), "sysinfo").await
    }

    /// Authenticated user info.
    ///
    /// # Errors
    /// See [`UnifiError`] for the failure cases this can return.
    pub async fn me(&self) -> Result<Value> {
        self.get(self.self_path(), "me").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips_a_non_default_request_timeout() {
        let cfg = UnifiConfig {
            url: "https://unifi.local".to_string(),
            api_key: "test-key".to_string(),
            request_timeout: Duration::from_secs(90),
            ..UnifiConfig::default()
        };

        let client = UnifiClient::new(&cfg).unwrap();

        assert_eq!(client.config().request_timeout, Duration::from_secs(90));
    }
}
