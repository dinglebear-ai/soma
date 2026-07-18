use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Default [`GotifyConfig::request_timeout`] when not otherwise specified.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Gotify server connection config.
///
/// `Debug` redacts [`client_token`](Self::client_token) and
/// [`app_token`](Self::app_token) so this can't leak into logs or traces
/// through an incidental `{:?}` — only each token's length is shown.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GotifyConfig {
    /// Server base URL, e.g. `https://gotify.example.com` (`GOTIFY_URL`).
    pub url: String,
    /// Client token for management operations: messages, applications,
    /// clients, current user (`GOTIFY_CLIENT_TOKEN`). Create one under
    /// **Clients** in the Gotify web UI. Empty means unconfigured — calls
    /// that need it return [`crate::GotifyError::MissingClientToken`].
    pub client_token: String,
    /// App token for sending messages (`GOTIFY_APP_TOKEN`). Distinct from
    /// `client_token` — create one under **Applications** in the Gotify web
    /// UI. Empty means unconfigured — [`crate::GotifyClient::send_message`]
    /// returns [`crate::GotifyError::MissingAppToken`].
    pub app_token: String,
    /// Per-request timeout, applied to the pooled `reqwest::Client` at
    /// construction. Defaults to [`DEFAULT_REQUEST_TIMEOUT`].
    #[serde(with = "duration_secs")]
    pub request_timeout: Duration,
}

impl Default for GotifyConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            client_token: String::new(),
            app_token: String::new(),
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }
}

impl fmt::Debug for GotifyConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GotifyConfig")
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

/// (De)serializes a [`Duration`] as whole seconds. `std::time::Duration` has
/// no built-in `serde` support, and pulling in `serde_with`/`humantime-serde`
/// for one field would be a heavier dependency than this crate's single
/// duration field justifies.
mod duration_secs {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(value.as_secs())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Duration, D::Error> {
        Ok(Duration::from_secs(u64::deserialize(deserializer)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_both_tokens() {
        let cfg = GotifyConfig {
            client_token: "super-secret-client".to_string(),
            app_token: "super-secret-app".to_string(),
            ..GotifyConfig::default()
        };

        let debug = format!("{cfg:?}");

        assert!(!debug.contains("super-secret-client"));
        assert!(!debug.contains("super-secret-app"));
        assert!(debug.contains("redacted"));
    }

    #[test]
    fn request_timeout_serializes_as_whole_seconds() {
        let cfg = GotifyConfig {
            request_timeout: Duration::from_secs(90),
            ..GotifyConfig::default()
        };

        let json = serde_json::to_value(&cfg).unwrap();

        assert_eq!(json["request_timeout"], serde_json::json!(90));
    }

    #[test]
    fn request_timeout_round_trips_through_json() {
        let cfg = GotifyConfig {
            request_timeout: Duration::from_secs(90),
            ..GotifyConfig::default()
        };

        let json = serde_json::to_string(&cfg).unwrap();
        let restored: GotifyConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.request_timeout, Duration::from_secs(90));
    }
}
