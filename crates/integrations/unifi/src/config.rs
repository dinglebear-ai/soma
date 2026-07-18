use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Default [`UnifiConfig::request_timeout`] when not otherwise specified.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// UniFi controller connection config.
///
/// `Debug` redacts [`api_key`](Self::api_key) so this can't leak into logs or
/// traces through an incidental `{:?}` — only its length is shown.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UnifiConfig {
    /// Controller base URL, e.g. `https://unifi.local` (`UNIFI_URL`).
    pub url: String,
    /// API key for the `X-API-KEY` header (`UNIFI_API_KEY`).
    pub api_key: String,
    /// Site name (`UNIFI_SITE`, default `"default"`).
    pub site: String,
    /// Skip TLS certificate verification. Defaults to `false` (verify) —
    /// self-signed local UniFi controllers need this explicitly set to
    /// `true`; a client should never silently accept invalid certificates.
    pub skip_tls_verify: bool,
    /// Legacy controller mode: no `/proxy/network` prefix, typically port 8443.
    pub legacy: bool,
    /// Per-request timeout, applied to the pooled `reqwest::Client` at
    /// construction. Defaults to [`DEFAULT_REQUEST_TIMEOUT`]; override for
    /// controllers or actions (large exports, slow WAN links) that
    /// routinely need longer than 30s.
    #[serde(with = "duration_secs")]
    pub request_timeout: Duration,
}

impl Default for UnifiConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            api_key: String::new(),
            site: "default".to_string(),
            skip_tls_verify: false,
            legacy: false,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }
}

impl fmt::Debug for UnifiConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnifiConfig")
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

/// (De)serializes a [`Duration`] as whole seconds. `std::time::Duration`
/// has no built-in `serde` support, and pulling in `serde_with` or
/// `humantime-serde` for one field would be a heavier dependency than this
/// crate's single duration field justifies.
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
    fn debug_redacts_api_key() {
        let cfg = UnifiConfig {
            api_key: "super-secret-key".to_string(),
            ..UnifiConfig::default()
        };

        let debug = format!("{cfg:?}");

        assert!(!debug.contains("super-secret-key"));
        assert!(debug.contains("redacted"));
    }

    #[test]
    fn request_timeout_serializes_as_whole_seconds() {
        let cfg = UnifiConfig {
            request_timeout: Duration::from_secs(90),
            ..UnifiConfig::default()
        };

        let json = serde_json::to_value(&cfg).unwrap();

        assert_eq!(json["request_timeout"], serde_json::json!(90));
    }

    #[test]
    fn request_timeout_round_trips_through_json() {
        let cfg = UnifiConfig {
            request_timeout: Duration::from_secs(90),
            ..UnifiConfig::default()
        };

        let json = serde_json::to_string(&cfg).unwrap();
        let restored: UnifiConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.request_timeout, Duration::from_secs(90));
    }
}
