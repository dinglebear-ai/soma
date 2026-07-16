use std::fs;
use std::io::Write;
use std::path::Path;

use crate::config::{ConfigError, GatewayConfig, GatewayPaths};

#[derive(Debug, Clone)]
pub struct FsGatewayConfigStore {
    paths: GatewayPaths,
}

impl FsGatewayConfigStore {
    pub fn new(home: std::path::PathBuf) -> Self {
        let paths = GatewayPaths::new(home).expect("valid gateway home");
        Self { paths }
    }

    pub fn from_paths(paths: GatewayPaths) -> Self {
        Self { paths }
    }

    #[must_use]
    pub fn paths(&self) -> &GatewayPaths {
        &self.paths
    }

    pub fn install_default(&self) -> Result<GatewayConfig, ConfigError> {
        let cfg = GatewayConfig::default();
        self.save(&cfg)?;
        Ok(cfg)
    }

    pub fn load_or_install_default(&self) -> Result<GatewayConfig, ConfigError> {
        if self.paths.config_path().exists() {
            self.load()
        } else {
            self.install_default()
        }
    }

    pub fn load(&self) -> Result<GatewayConfig, ConfigError> {
        let config_path = self.paths.config_path();
        let raw =
            fs::read_to_string(&config_path).map_err(|err| ConfigError::io(&config_path, err))?;
        let cfg: GatewayConfig = toml::from_str(&raw)?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn save(&self, cfg: &GatewayConfig) -> Result<(), ConfigError> {
        cfg.validate()?;
        let config_path = self.paths.config_path();
        write_file_atomically(&config_path, &toml::to_string_pretty(cfg)?, false)
    }

    pub fn write_env_secret(&self, key: &str, value: &str) -> Result<(), ConfigError> {
        crate::config::upstream::validate_bearer_token_env(key)?;
        let env_path = self.paths.env_path();
        let mut entries = parse_env_file(&env_path)?;
        entries.retain(|(existing, _)| existing != key);
        entries.push((key.to_owned(), value.to_owned()));
        let rendered = entries
            .into_iter()
            .map(|(key, value)| format!("{key}={}\n", quote_env_value(&value)))
            .collect::<String>();
        write_file_atomically(&env_path, &rendered, true)
    }
}

fn write_file_atomically(path: &Path, body: &str, secret: bool) -> Result<(), ConfigError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|err| ConfigError::io(parent, err))?;
    reject_symlink(path)?;

    let tmp = parent.join(format!(
        ".{}.tmp.{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file"),
        std::process::id()
    ));
    {
        let mut file = fs::File::create(&tmp).map_err(|err| ConfigError::io(&tmp, err))?;
        file.write_all(body.as_bytes())
            .map_err(|err| ConfigError::io(&tmp, err))?;
        file.sync_all().map_err(|err| ConfigError::io(&tmp, err))?;
    }
    if secret {
        restrict_secret_file_permissions(&tmp)?;
    }
    fs::rename(&tmp, path).map_err(|err| ConfigError::io(path, err))?;
    if secret {
        restrict_secret_file_permissions(path)?;
    }
    Ok(())
}

fn parse_env_file(path: &Path) -> Result<Vec<(String, String)>, ConfigError> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(ConfigError::io(path, err)),
    };
    Ok(raw
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            trimmed
                .split_once('=')
                .map(|(key, value)| (key.trim().to_owned(), unquote_env_value(value.trim())))
        })
        .collect())
}

fn reject_symlink(path: &Path) -> Result<(), ConfigError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(ConfigError::invalid("path", "must not be a symlink"))
        }
        Ok(_) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(ConfigError::io(path, err)),
    }
}

fn quote_env_value(value: &str) -> String {
    let needs_quotes = value
        .chars()
        .any(|ch| matches!(ch, ' ' | '\t' | '#' | '$' | '\\' | '"' | '\'' | '`'));
    if needs_quotes {
        format!("\"{}\"", value.replace('\\', r"\\").replace('"', r#"\""#))
    } else {
        value.to_owned()
    }
}

fn unquote_env_value(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .map_or_else(
            || value.to_owned(),
            |inner| inner.replace(r#"\""#, "\"").replace(r"\\", r"\"),
        )
}

fn restrict_secret_file_permissions(path: &Path) -> Result<(), ConfigError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|err| ConfigError::io(path, err))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
#[path = "config_store_tests.rs"]
mod tests;
