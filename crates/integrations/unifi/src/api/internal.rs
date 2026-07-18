//! Path/URL construction for the UniFi controller's internal (web-UI) API.

/// Builds paths and URLs under a controller's internal, per-site API.
#[derive(Debug, Clone)]
pub struct InternalNetworkApi {
    base_url: String,
    site: String,
    legacy: bool,
}

impl InternalNetworkApi {
    /// `base_url` is the controller's base URL (trailing slash trimmed);
    /// `site` is the site slug; `legacy` selects controllers without the
    /// `/proxy/network` prefix.
    pub fn new(base_url: impl Into<String>, site: impl Into<String>, legacy: bool) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            site: site.into(),
            legacy,
        }
    }

    /// Builds a `v1`-style per-site path: `/api/s/{site}/{suffix}`, prefixed
    /// with `/proxy/network` unless this is a legacy controller.
    pub fn v1_site_path(&self, suffix: &str) -> String {
        let suffix = suffix.trim_start_matches('/');
        let prefix = if self.legacy { "" } else { "/proxy/network" };
        format!("{prefix}/api/s/{site}/{suffix}", site = self.site)
    }

    /// Builds a `v2`-style per-site path: `/v2/api/site/{site}/{suffix}`,
    /// prefixed with `/proxy/network` unless this is a legacy controller.
    pub fn v2_site_path(&self, suffix: &str) -> String {
        let suffix = suffix.trim_start_matches('/');
        if self.legacy {
            format!("/v2/api/site/{site}/{suffix}", site = self.site)
        } else {
            format!(
                "/proxy/network/v2/api/site/{site}/{suffix}",
                site = self.site
            )
        }
    }

    /// `path`, joined onto this API's base URL as-is (no prefixing).
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_site_path_adds_the_proxy_prefix_for_non_legacy_controllers() {
        let api = InternalNetworkApi::new("https://unifi.local", "default", false);

        assert_eq!(
            api.v1_site_path("stat/sta"),
            "/proxy/network/api/s/default/stat/sta"
        );
    }

    #[test]
    fn v1_site_path_omits_the_proxy_prefix_for_legacy_controllers() {
        let api = InternalNetworkApi::new("https://unifi.local:8443", "default", true);

        assert_eq!(api.v1_site_path("/stat/sta"), "/api/s/default/stat/sta");
    }

    #[test]
    fn v2_site_path_adds_the_proxy_prefix_for_non_legacy_controllers() {
        let api = InternalNetworkApi::new("https://unifi.local", "default", false);

        assert_eq!(
            api.v2_site_path("system-log/all"),
            "/proxy/network/v2/api/site/default/system-log/all"
        );
    }

    #[test]
    fn v2_site_path_omits_the_proxy_prefix_for_legacy_controllers() {
        let api = InternalNetworkApi::new("https://unifi.local:8443", "default", true);

        assert_eq!(
            api.v2_site_path("system-log/all"),
            "/v2/api/site/default/system-log/all"
        );
    }
}
