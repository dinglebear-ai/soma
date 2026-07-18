//! Path/URL construction for UniFi's official `/proxy/network/integration` REST API.

/// Builds paths and URLs under a controller's official integration API.
#[derive(Debug, Clone)]
pub struct OfficialNetworkApi {
    base_url: String,
}

impl OfficialNetworkApi {
    /// `base_url` is the controller's base URL, e.g. `https://unifi.local`; a
    /// trailing slash is trimmed.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    /// Maps a capability-catalog path (e.g. `v1/sites`) onto the official
    /// integration API's request path.
    ///
    /// Connector actions (`ConnectorGet`/`Post`/`Put`/`Patch`/`Delete`) can
    /// substitute a `*path` wildcard that is already fully qualified under
    /// `/proxy/network/integration/` or `/proxy/protect/integration/` (see
    /// [`crate::api::path::validate_connector_path`]) — pass those through
    /// unchanged rather than prefixing them a second time.
    pub fn path(&self, path: &str) -> String {
        if path.starts_with("/proxy/network/integration/")
            || path.starts_with("/proxy/protect/integration/")
        {
            return path.to_string();
        }
        let normalized = path.trim_start_matches('/');
        if let Some(rest) = normalized.strip_prefix("v1/") {
            format!("/proxy/network/integration/v1/{rest}")
        } else {
            format!("/proxy/network/integration/{normalized}")
        }
    }

    /// [`Self::path`], joined onto this API's base URL.
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, self.path(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_prefixes_v1_routes_with_the_integration_v1_segment() {
        let api = OfficialNetworkApi::new("https://unifi.local");

        assert_eq!(api.path("v1/sites"), "/proxy/network/integration/v1/sites");
    }

    #[test]
    fn path_prefixes_other_routes_without_a_version_segment() {
        let api = OfficialNetworkApi::new("https://unifi.local");

        assert_eq!(
            api.path("connectors"),
            "/proxy/network/integration/connectors"
        );
    }

    #[test]
    fn path_passes_through_an_already_qualified_network_connector_path() {
        let api = OfficialNetworkApi::new("https://unifi.local");

        assert_eq!(
            api.path("/proxy/network/integration/v1/sites"),
            "/proxy/network/integration/v1/sites"
        );
    }

    #[test]
    fn path_passes_through_an_already_qualified_protect_connector_path() {
        let api = OfficialNetworkApi::new("https://unifi.local");

        assert_eq!(
            api.path("/proxy/protect/integration/v1/cameras"),
            "/proxy/protect/integration/v1/cameras"
        );
    }

    #[test]
    fn url_joins_the_base_url_and_path() {
        let api = OfficialNetworkApi::new("https://unifi.local/");

        assert_eq!(
            api.url("v1/sites"),
            "https://unifi.local/proxy/network/integration/v1/sites"
        );
    }
}
