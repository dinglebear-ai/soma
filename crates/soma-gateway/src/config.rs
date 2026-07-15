//! Gateway configuration DTOs and local persistence-safe views.

pub mod defaults;
pub mod protected_routes;
pub mod upstream;
pub mod virtual_servers;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use defaults::GatewayPaths;
pub use protected_routes::{ProtectedGatewaySubsetTarget, ProtectedMcpRouteConfig};
pub use upstream::{
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
            protected_mcp_routes: self.protected_mcp_routes.clone(),
            virtual_servers: self.virtual_servers.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayConfigView {
    pub upstream: Vec<UpstreamConfigView>,
    pub protected_mcp_routes: Vec<ProtectedMcpRouteConfig>,
    pub virtual_servers: Vec<VirtualServerConfig>,
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
