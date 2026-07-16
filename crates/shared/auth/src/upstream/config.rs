//! Minimal, self-contained upstream configuration shape.
//!
//! soma-auth intentionally does not depend on any gateway/runtime crate for
//! outbound upstream OAuth. Only the fields the OAuth runtime actually reads
//! (`name`, `url`, `oauth`) are modeled here — a full gateway config schema
//! (tool/resource exposure allowlists, proxy flags, priority, import
//! provenance, ...) belongs to whatever consumer wires this runtime up, not
//! to the auth crate itself.

use serde::{Deserialize, Serialize};

/// Upstream MCP server identity + outbound OAuth configuration, as needed by
/// [`crate::upstream::manager::UpstreamOauthManager`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamConfig {
    /// Human-readable name for this upstream (used as the OAuth manager/cache key).
    pub name: String,
    /// URL of the upstream MCP server. Required when `oauth` is set.
    #[serde(default)]
    pub url: Option<String>,
    /// Outbound OAuth configuration. `None` means no OAuth manager is built
    /// for this upstream.
    #[serde(default)]
    pub oauth: Option<UpstreamOauthConfig>,
}

impl UpstreamConfig {
    /// Canonicalized `url` per RFC 3986 §6.2.2 (scheme/host lowercase,
    /// default port stripped, dot-segment removal, percent-encoding case
    /// normalization). Trailing slashes are preserved — they are
    /// semantically significant in HTTP paths.
    ///
    /// Returns `None` when `url` is unset, `Some(Err(_))` when it is set but
    /// not a valid URL.
    #[must_use]
    pub fn canonical_url(&self) -> Option<Result<String, url::ParseError>> {
        self.url
            .as_deref()
            .map(|raw| url::Url::parse(raw.trim()).map(|parsed| parsed.to_string()))
    }
}

/// Outbound OAuth configuration for a single upstream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamOauthConfig {
    pub mode: UpstreamOauthMode,
    pub registration: UpstreamOauthRegistration,
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
    /// When `true`, always use the Client ID Metadata Document (CIMD)
    /// strategy regardless of whether the upstream advertises a
    /// `registration_endpoint`. When `false`, always use dynamic
    /// registration (RFC 7591) when the upstream advertises a
    /// `registration_endpoint`. `None` leaves the choice to the caller
    /// wiring up [`UpstreamOauthRegistration::Dynamic`].
    #[serde(default)]
    pub prefer_client_metadata_document: Option<bool>,
}

/// Outbound OAuth mode. Currently only `authorization_code_pkce` is supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamOauthMode {
    AuthorizationCodePkce,
}

/// Outbound OAuth client-registration strategy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub enum UpstreamOauthRegistration {
    ClientMetadataDocument {
        url: String,
    },
    Preregistered {
        client_id: String,
        #[serde(default)]
        client_secret_env: Option<String>,
    },
    Dynamic,
}
