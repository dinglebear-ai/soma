//! Gateway configuration DTOs and local persistence-safe views.

pub mod defaults;
pub mod protected_routes;
pub mod virtual_servers;

pub mod upstream {
    pub use soma_mcp_client::config::*;
}

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use defaults::GatewayPaths;
pub use protected_routes::{ProtectedGatewaySubsetTarget, ProtectedMcpRouteConfig};
pub use soma_mcp_client::config::{
    GatewayUpstreamOauthConfig, GatewayUpstreamOauthMode, GatewayUpstreamOauthRegistration,
    UpstreamConfig, UpstreamConfigView,
};
pub use virtual_servers::VirtualServerConfig;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("{field}: {message}")]
    InvalidField {
        field: &'static str,
        message: String,
    },
    #[error("io error while handling {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("toml serialization error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
    #[error("toml parse error: {0}")]
    TomlDeserialize(#[from] toml::de::Error),
}

impl ConfigError {
    pub(crate) fn invalid(field: &'static str, message: impl Into<String>) -> Self {
        Self::InvalidField {
            field,
            message: message.into(),
        }
    }

    pub(crate) fn io(path: &std::path::Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.display().to_string(),
            source,
        }
    }
}

impl From<soma_mcp_client::ConfigError> for ConfigError {
    fn from(error: soma_mcp_client::ConfigError) -> Self {
        match error {
            soma_mcp_client::ConfigError::InvalidField { field, message } => {
                Self::InvalidField { field, message }
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayConfig {
    #[serde(default)]
    pub upstream: Vec<UpstreamConfig>,
    #[serde(default)]
    pub protected_mcp_routes: Vec<ProtectedMcpRouteConfig>,
    #[serde(default)]
    pub virtual_servers: Vec<VirtualServerConfig>,
}

impl GatewayConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        for upstream in &self.upstream {
            upstream.validate()?;
        }
        for route in &self.protected_mcp_routes {
            route.validate()?;
        }
        for server in &self.virtual_servers {
            server.validate()?;
        }
        Ok(())
    }

    #[must_use]
    pub fn redacted_view(&self) -> GatewayConfigView {
        GatewayConfigView {
            upstream: self
                .upstream
                .iter()
                .map(UpstreamConfig::redacted_view)
                .collect(),
            protected_mcp_routes: self
                .protected_mcp_routes
                .iter()
                .map(ProtectedMcpRouteConfigView::from)
                .collect(),
            virtual_servers: self.virtual_servers.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayConfigView {
    pub upstream: Vec<UpstreamConfigView>,
    pub protected_mcp_routes: Vec<ProtectedMcpRouteConfigView>,
    pub virtual_servers: Vec<VirtualServerConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtectedMcpRouteConfigView {
    pub name: String,
    pub enabled: bool,
    pub public_host: String,
    pub public_path: String,
    pub upstream: Option<String>,
    pub has_backend_url: bool,
    pub scopes: Vec<String>,
    pub target: Option<ProtectedGatewaySubsetTarget>,
}

impl From<&ProtectedMcpRouteConfig> for ProtectedMcpRouteConfigView {
    fn from(route: &ProtectedMcpRouteConfig) -> Self {
        Self {
            name: route.name.clone(),
            enabled: route.enabled,
            public_host: route.public_host.clone(),
            public_path: route.public_path.clone(),
            upstream: route.upstream.clone(),
            has_backend_url: !route.backend_url.trim().is_empty(),
            scopes: route.scopes.clone(),
            target: route.target.clone(),
        }
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
