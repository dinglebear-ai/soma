use serde::{Deserialize, Serialize};

use super::ConfigError;
use crate::config::upstream::default_true;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtectedGatewaySubsetTarget {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upstreams: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<String>,
    #[serde(default)]
    pub expose_code_mode: bool,
}

fn default_protected_route_scopes() -> Vec<String> {
    Vec::new()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtectedMcpRouteConfig {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub public_host: String,
    pub public_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub backend_url: String,
    #[serde(default = "default_protected_route_scopes")]
    pub scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<ProtectedGatewaySubsetTarget>,
}

impl Default for ProtectedMcpRouteConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            enabled: true,
            public_host: String::new(),
            public_path: "/mcp".to_owned(),
            upstream: None,
            backend_url: String::new(),
            scopes: default_protected_route_scopes(),
            target: None,
        }
    }
}

impl ProtectedMcpRouteConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.name.trim().is_empty() {
            return Err(ConfigError::invalid("route.name", "must not be empty"));
        }
        if normalize_public_host(&self.public_host).is_empty() {
            return Err(ConfigError::invalid(
                "route.public_host",
                "must not be empty",
            ));
        }
        if self.public_host.contains(',') {
            return Err(ConfigError::invalid(
                "route.public_host",
                "must contain exactly one host",
            ));
        }
        if !self.public_path.starts_with('/') {
            return Err(ConfigError::invalid(
                "route.public_path",
                "must start with /",
            ));
        }
        if !self.backend_url.trim().is_empty() {
            crate::security::ssrf::validate_url(
                &self.backend_url,
                crate::security::ssrf::OutboundPolicy::AdminProtectedBackend,
            )
            .map_err(|_| ConfigError::invalid("route.backend_url", "is not allowed"))?;
        }
        if self.scopes.iter().any(|scope| scope.trim().is_empty()) {
            return Err(ConfigError::invalid(
                "route.scopes",
                "must not contain empty scopes",
            ));
        }
        if let Some(target) = &self.target {
            if target
                .upstreams
                .iter()
                .any(|upstream| upstream.trim().is_empty())
            {
                return Err(ConfigError::invalid(
                    "route.target.upstreams",
                    "must not contain empty upstream names",
                ));
            }
            if target
                .services
                .iter()
                .any(|service| service.trim().is_empty())
            {
                return Err(ConfigError::invalid(
                    "route.target.services",
                    "must not contain empty service names",
                ));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn public_resource(&self) -> String {
        format!(
            "https://{}{}",
            normalize_public_host(&self.public_host),
            self.public_path
        )
    }
}

#[must_use]
pub fn normalize_public_host(host: &str) -> String {
    host.trim().trim_end_matches('.').to_ascii_lowercase()
}

#[cfg(test)]
#[path = "protected_routes_tests.rs"]
mod tests;
