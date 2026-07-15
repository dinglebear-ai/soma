use std::path::{Path, PathBuf};

use super::ConfigError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayPaths {
    home: PathBuf,
}

impl GatewayPaths {
    pub fn new(home: PathBuf) -> Result<Self, ConfigError> {
        validate_soma_home(&home)?;
        Ok(Self { home })
    }

    pub fn from_env() -> Result<Self, ConfigError> {
        let home = std::env::var_os("SOMA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(default_soma_home);
        Self::new(home)
    }

    #[must_use]
    pub fn home(&self) -> &Path {
        &self.home
    }

    #[must_use]
    pub fn config_path(&self) -> PathBuf {
        self.home.join("config.toml")
    }

    #[must_use]
    pub fn env_path(&self) -> PathBuf {
        self.home.join(".env")
    }
}

fn default_soma_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".soma")
}

fn validate_soma_home(path: &Path) -> Result<(), ConfigError> {
    if path.as_os_str().is_empty() {
        return Err(ConfigError::invalid("soma_home", "must not be empty"));
    }
    if !path.is_absolute() {
        return Err(ConfigError::invalid("soma_home", "must be absolute"));
    }
    let leaf = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if leaf != ".soma" {
        return Err(ConfigError::invalid(
            "soma_home",
            "must point at a .soma directory",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "defaults_tests.rs"]
mod tests;
