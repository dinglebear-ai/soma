use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

use crate::{TopologyError, TopologyRevision};

const MAX_ENDPOINT_CHARS: usize = 512;
const MAX_USER_CHARS: usize = 128;

/// Transport endpoint for one managed host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostEndpoint {
    /// Commands execute on the current machine.
    Local,
    /// Commands execute through a strict-host-key SSH endpoint.
    Ssh(SshEndpoint),
    /// Operations route to an HTTP service endpoint.
    Http(HttpEndpoint),
}

impl HostEndpoint {
    pub(crate) fn revision(&self) -> TopologyRevision {
        let material = serde_json::to_vec(self).expect("fleet endpoints serialize");
        TopologyRevision::from_material(material)
    }
}

/// Strict-host-key SSH connection material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SshEndpoint {
    host: String,
    port: u16,
    user: Option<String>,
    identity_file: Option<PathBuf>,
    config_file: Option<PathBuf>,
    known_hosts_file: Option<PathBuf>,
}

impl SshEndpoint {
    /// Creates an SSH endpoint using port 22.
    pub fn new(host: impl Into<String>) -> Result<Self, TopologyError> {
        let host = host.into();
        validate_endpoint_text("SSH host", &host)?;
        Ok(Self {
            host,
            port: 22,
            user: None,
            identity_file: None,
            config_file: None,
            known_hosts_file: None,
        })
    }

    /// Sets the SSH port.
    pub fn with_port(mut self, port: u16) -> Result<Self, TopologyError> {
        if port == 0 {
            return Err(TopologyError::InvalidPort);
        }
        self.port = port;
        Ok(self)
    }

    /// Sets the optional SSH user.
    pub fn with_user(mut self, user: impl Into<String>) -> Result<Self, TopologyError> {
        let user = user.into();
        validate_bounded_text("SSH user", &user, MAX_USER_CHARS)?;
        self.user = Some(user);
        Ok(self)
    }

    /// Sets an absolute identity-file path.
    pub fn with_identity_file(mut self, path: impl Into<PathBuf>) -> Result<Self, TopologyError> {
        self.identity_file = Some(validate_absolute_path(path.into())?);
        Ok(self)
    }

    /// Sets an absolute SSH config path.
    pub fn with_config_file(mut self, path: impl Into<PathBuf>) -> Result<Self, TopologyError> {
        self.config_file = Some(validate_absolute_path(path.into())?);
        Ok(self)
    }

    /// Sets an absolute known-hosts path used with strict verification.
    pub fn with_known_hosts_file(
        mut self,
        path: impl Into<PathBuf>,
    ) -> Result<Self, TopologyError> {
        self.known_hosts_file = Some(validate_absolute_path(path.into())?);
        Ok(self)
    }

    /// Returns the SSH hostname or config alias.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns the SSH port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// Returns the optional SSH user.
    #[must_use]
    pub fn user(&self) -> Option<&str> {
        self.user.as_deref()
    }

    /// Returns the optional identity-file path.
    #[must_use]
    pub fn identity_file(&self) -> Option<&Path> {
        self.identity_file.as_deref()
    }

    /// Returns the optional config-file path.
    #[must_use]
    pub fn config_file(&self) -> Option<&Path> {
        self.config_file.as_deref()
    }

    /// Returns the optional strict known-hosts path.
    #[must_use]
    pub fn known_hosts_file(&self) -> Option<&Path> {
        self.known_hosts_file.as_deref()
    }
}

impl<'de> Deserialize<'de> for SshEndpoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = SshEndpointWire::deserialize(deserializer)?;
        let mut endpoint = Self::new(wire.host).map_err(serde::de::Error::custom)?;
        endpoint = endpoint
            .with_port(wire.port)
            .map_err(serde::de::Error::custom)?;
        if let Some(user) = wire.user {
            endpoint = endpoint.with_user(user).map_err(serde::de::Error::custom)?;
        }
        if let Some(path) = wire.identity_file {
            endpoint = endpoint
                .with_identity_file(path)
                .map_err(serde::de::Error::custom)?;
        }
        if let Some(path) = wire.config_file {
            endpoint = endpoint
                .with_config_file(path)
                .map_err(serde::de::Error::custom)?;
        }
        if let Some(path) = wire.known_hosts_file {
            endpoint = endpoint
                .with_known_hosts_file(path)
                .map_err(serde::de::Error::custom)?;
        }
        Ok(endpoint)
    }
}

#[derive(Deserialize)]
struct SshEndpointWire {
    host: String,
    #[serde(default = "default_ssh_port")]
    port: u16,
    #[serde(default)]
    user: Option<String>,
    #[serde(default)]
    identity_file: Option<PathBuf>,
    #[serde(default)]
    config_file: Option<PathBuf>,
    #[serde(default)]
    known_hosts_file: Option<PathBuf>,
}

const fn default_ssh_port() -> u16 {
    22
}

/// Validated HTTP or HTTPS fleet endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HttpEndpoint {
    base_url: String,
}

impl HttpEndpoint {
    /// Creates an HTTP endpoint without embedded credentials.
    pub fn new(base_url: impl Into<String>) -> Result<Self, TopologyError> {
        let base_url = base_url.into();
        validate_endpoint_text("HTTP base URL", &base_url)?;
        let Some((scheme, remainder)) = base_url.split_once("://") else {
            return Err(TopologyError::InvalidHttpEndpoint);
        };
        if !matches!(scheme, "http" | "https") {
            return Err(TopologyError::InvalidHttpEndpoint);
        }
        let authority = remainder.split('/').next().unwrap_or_default();
        if authority.is_empty() || authority.contains('@') {
            return Err(TopologyError::InvalidHttpEndpoint);
        }
        Ok(Self { base_url })
    }

    /// Returns the normalized endpoint text.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl<'de> Deserialize<'de> for HttpEndpoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = HttpEndpointWire::deserialize(deserializer)?;
        Self::new(wire.base_url).map_err(serde::de::Error::custom)
    }
}

#[derive(Deserialize)]
struct HttpEndpointWire {
    base_url: String,
}

fn validate_endpoint_text(field: &'static str, value: &str) -> Result<(), TopologyError> {
    validate_bounded_text(field, value, MAX_ENDPOINT_CHARS)
}

fn validate_bounded_text(
    field: &'static str,
    value: &str,
    max_chars: usize,
) -> Result<(), TopologyError> {
    let count = value.chars().count();
    if count == 0 || count > max_chars || value.chars().any(char::is_control) {
        Err(TopologyError::InvalidEndpointText { field })
    } else {
        Ok(())
    }
}

fn validate_absolute_path(path: PathBuf) -> Result<PathBuf, TopologyError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        Err(TopologyError::InvalidAbsolutePath(path))
    } else {
        Ok(path)
    }
}

#[cfg(test)]
#[path = "endpoint_tests.rs"]
mod tests;
