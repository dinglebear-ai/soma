use serde::{Deserialize, Serialize};

use super::ConfigError;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualServerConfig {
    pub id: String,
    pub service: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub surfaces: VirtualServerSurfacesConfig,
    #[serde(default)]
    pub mcp_policy: Option<VirtualServerMcpPolicyConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualServerSurfacesConfig {
    #[serde(default)]
    pub cli: bool,
    #[serde(default)]
    pub api: bool,
    #[serde(default)]
    pub mcp: bool,
    #[serde(default)]
    pub webui: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualServerMcpPolicyConfig {
    #[serde(default)]
    pub allowed_actions: Vec<String>,
}

impl VirtualServerConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.id.trim().is_empty() {
            return Err(ConfigError::invalid(
                "virtual_server.id",
                "must not be empty",
            ));
        }
        if self.service.trim().is_empty() {
            return Err(ConfigError::invalid(
                "virtual_server.service",
                "must not be empty",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "virtual_servers_tests.rs"]
mod tests;
