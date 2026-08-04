use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

const DEFAULT_PORT: u16 = 40070;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SynapseConfig {
    pub server: ServerConfig,
    pub hosts: Vec<HostConfig>,
}

impl Default for SynapseConfig {
    fn default() -> Self {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        Self {
            server: ServerConfig::default(),
            hosts: vec![HostConfig::local("local", root)],
        }
    }
}

impl SynapseConfig {
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        let explicit = path.is_some();
        let path = path.map(Path::to_path_buf).or_else(default_config_path);
        let Some(path) = path else {
            return Ok(Self::default());
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).map_err(|error| {
                anyhow::anyhow!("invalid Synapse config {}: {error}", path.display())
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !explicit => {
                Ok(Self::default())
            }
            Err(error) => Err(anyhow::anyhow!(
                "cannot read Synapse config {}: {error}",
                path.display()
            )),
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.hosts.is_empty() {
            anyhow::bail!("at least one host must be configured");
        }
        let mut ids = std::collections::BTreeSet::new();
        for host in &self.hosts {
            if !ids.insert(host.id.as_str()) {
                anyhow::bail!("duplicate host id: {}", host.id);
            }
            host.validate()?;
        }
        if let Some(default) = self.server.default_host.as_deref()
            && !ids.contains(default)
        {
            anyhow::bail!("default host is not configured: {default}");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub bind: String,
    pub api_token: Option<String>,
    pub allow_mutations: bool,
    pub request_timeout_secs: u64,
    pub authorization_ttl_secs: u64,
    pub default_host: Option<String>,
    pub max_fanout_concurrency: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: format!("127.0.0.1:{DEFAULT_PORT}"),
            api_token: None,
            allow_mutations: false,
            request_timeout_secs: 30,
            authorization_ttl_secs: 60,
            default_host: Some("local".into()),
            max_fanout_concurrency: 8,
        }
    }
}

impl ServerConfig {
    pub fn bind_addr(&self) -> anyhow::Result<SocketAddr> {
        self.bind
            .parse()
            .map_err(|error| anyhow::anyhow!("invalid server.bind {}: {error}", self.bind))
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_secs.clamp(1, 3_600))
    }

    pub fn authorization_ttl(&self) -> Duration {
        Duration::from_secs(self.authorization_ttl_secs.clamp(1, 3_600))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostConfig {
    pub id: String,
    pub endpoint: EndpointConfig,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub read_roots: Vec<PathBuf>,
    #[serde(default)]
    pub build_roots: Vec<PathBuf>,
    #[serde(default)]
    pub transfer_source_roots: Vec<PathBuf>,
    #[serde(default)]
    pub transfer_destination_roots: Vec<PathBuf>,
    #[serde(default)]
    pub docker_socket: Option<PathBuf>,
}

impl HostConfig {
    fn local(id: &str, root: PathBuf) -> Self {
        Self {
            id: id.into(),
            endpoint: EndpointConfig::Local,
            labels: Vec::new(),
            read_roots: vec![root.clone()],
            build_roots: vec![root.clone()],
            transfer_source_roots: vec![root.clone()],
            transfer_destination_roots: vec![root],
            docker_socket: None,
        }
    }

    fn validate(&self) -> anyhow::Result<()> {
        if self.read_roots.is_empty() {
            anyhow::bail!("host {} requires at least one read root", self.id);
        }
        for (name, roots) in [
            ("read_roots", &self.read_roots),
            ("build_roots", &self.build_roots),
            ("transfer_source_roots", &self.transfer_source_roots),
            (
                "transfer_destination_roots",
                &self.transfer_destination_roots,
            ),
        ] {
            for root in roots {
                if !root.is_absolute() {
                    anyhow::bail!(
                        "host {} {name} must be absolute: {}",
                        self.id,
                        root.display()
                    );
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EndpointConfig {
    Local,
    Ssh {
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
    },
}

const fn default_ssh_port() -> u16 {
    22
}

fn default_config_path() -> Option<PathBuf> {
    std::env::var_os("SYNAPSE_CONFIG")
        .map(PathBuf::from)
        .or_else(|| dirs::config_dir().map(|dir| dir.join("synapse/config.toml")))
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
