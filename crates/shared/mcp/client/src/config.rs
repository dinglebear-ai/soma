use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::ConfigError;
use crate::process::guard::SpawnGuard;
use crate::process::stdio::StdioProcessSpec;
use crate::security::redact::{is_sensitive_key, redact_stdio_args, redact_url};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamConfig {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub bearer_token_env: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub oauth: Option<GatewayUpstreamOauthConfig>,
    #[serde(default = "default_true")]
    pub proxy_resources: bool,
    #[serde(default = "default_true")]
    pub proxy_prompts: bool,
    #[serde(default)]
    pub expose_tools: Option<Vec<String>>,
    #[serde(default)]
    pub expose_resources: Option<Vec<String>>,
    #[serde(default)]
    pub expose_prompts: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamConfigView {
    pub name: String,
    pub enabled: bool,
    pub url: Option<String>,
    pub bearer_token_env: Option<String>,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env_keys: Vec<String>,
    pub oauth_enabled: bool,
    pub proxy_resources: bool,
    pub proxy_prompts: bool,
    pub expose_tools: Option<Vec<String>>,
    pub expose_resources: Option<Vec<String>>,
    pub expose_prompts: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewayUpstreamOauthConfig {
    pub mode: GatewayUpstreamOauthMode,
    pub registration: GatewayUpstreamOauthRegistration,
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
    #[serde(default)]
    pub prefer_client_metadata_document: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayUpstreamOauthMode {
    AuthorizationCodePkce,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub enum GatewayUpstreamOauthRegistration {
    ClientMetadataDocument {
        url: String,
    },
    Preregistered {
        client_id: String,
        #[serde(default)]
        client_secret_env: Option<String>,
    },
    Dynamic,
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            enabled: true,
            url: None,
            bearer_token_env: None,
            command: None,
            args: Vec::new(),
            env: BTreeMap::new(),
            oauth: None,
            proxy_resources: true,
            proxy_prompts: true,
            expose_tools: None,
            expose_resources: None,
            expose_prompts: None,
        }
    }
}

impl UpstreamConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_name(&self.name)?;
        validate_transport_shape(self)?;
        if let Some(name) = self.bearer_token_env.as_deref() {
            validate_bearer_token_env(name)?;
        }
        if let Some(url) = self.url.as_deref() {
            let parsed = url::Url::parse(url)
                .map_err(|_| ConfigError::invalid("url", "must be a valid URL"))?;
            match parsed.scheme() {
                "http" | "https" | "ws" | "wss" => {}
                _ => {
                    return Err(ConfigError::invalid(
                        "url",
                        "must use http, https, ws, or wss",
                    ));
                }
            }
        }
        if let Some(oauth) = &self.oauth {
            oauth.validate()?;
            if self.url.is_none() {
                return Err(ConfigError::invalid("oauth", "requires upstream url"));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn redacted_view(&self) -> UpstreamConfigView {
        UpstreamConfigView {
            name: self.name.clone(),
            enabled: self.enabled,
            url: self.url.as_deref().map(redact_url),
            bearer_token_env: self
                .bearer_token_env
                .as_ref()
                .map(|_| "[redacted]".to_owned()),
            command: self.command.clone(),
            args: redact_stdio_args(&self.args),
            env_keys: self
                .env
                .keys()
                .map(|key| {
                    if is_sensitive_key(key) {
                        "[redacted]".to_owned()
                    } else {
                        key.clone()
                    }
                })
                .collect(),
            oauth_enabled: self.oauth.is_some(),
            proxy_resources: self.proxy_resources,
            proxy_prompts: self.proxy_prompts,
            expose_tools: self.expose_tools.clone(),
            expose_resources: self.expose_resources.clone(),
            expose_prompts: self.expose_prompts.clone(),
        }
    }
}

fn validate_transport_shape(config: &UpstreamConfig) -> Result<(), ConfigError> {
    let has_url = config
        .url
        .as_deref()
        .map(str::trim)
        .is_some_and(|url| !url.is_empty());
    let has_command = config
        .command
        .as_deref()
        .map(str::trim)
        .is_some_and(|command| !command.is_empty());
    match (has_url, has_command) {
        (true, true) => {
            return Err(ConfigError::invalid(
                "transport",
                "must specify either url or command, not both",
            ));
        }
        (false, false) => {
            return Err(ConfigError::invalid(
                "transport",
                "must specify exactly one of url or command",
            ));
        }
        _ => {}
    }
    if has_command {
        let spec = StdioProcessSpec {
            command: config.command.clone().unwrap_or_default(),
            args: config.args.clone(),
            env: config.env.clone(),
        };
        spec.validate(&SpawnGuard::default())
            .map_err(|_| ConfigError::invalid("command", "stdio command is not allowed"))?;
    }
    Ok(())
}

impl GatewayUpstreamOauthConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if let Some(scopes) = &self.scopes {
            if scopes.iter().any(|scope| scope.trim().is_empty()) {
                return Err(ConfigError::invalid(
                    "oauth.scopes",
                    "must not contain blanks",
                ));
            }
        }
        if let GatewayUpstreamOauthRegistration::Preregistered {
            client_secret_env: Some(env),
            ..
        } = &self.registration
        {
            validate_bearer_token_env(env)?;
        }
        Ok(())
    }
}

pub fn default_true() -> bool {
    true
}

fn validate_name(name: &str) -> Result<(), ConfigError> {
    if name.trim().is_empty() {
        return Err(ConfigError::invalid("name", "must not be empty"));
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(ConfigError::invalid(
            "name",
            "must contain only ASCII letters, digits, hyphens, underscores, and dots",
        ));
    }
    Ok(())
}

pub fn validate_bearer_token_env(value: &str) -> Result<(), ConfigError> {
    let trimmed = value.trim();
    let looks_like_secret = trimmed.starts_with("Bearer ")
        || trimmed.starts_with("sk-")
        || trimmed.starts_with("ghp_")
        || trimmed.starts_with("github_pat_")
        || looks_like_jwt(trimmed);
    if looks_like_secret {
        return Err(ConfigError::invalid(
            "bearer_token_env",
            "must be an environment variable name, not a token value",
        ));
    }
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return Err(ConfigError::invalid(
            "bearer_token_env",
            "must not be empty",
        ));
    };
    if !(first == '_' || first.is_ascii_uppercase()) {
        return Err(ConfigError::invalid(
            "bearer_token_env",
            "must start with an uppercase ASCII letter or underscore",
        ));
    }
    if !chars.all(|ch| ch == '_' || ch.is_ascii_uppercase() || ch.is_ascii_digit()) {
        return Err(ConfigError::invalid(
            "bearer_token_env",
            "must contain only uppercase ASCII letters, digits, and underscores",
        ));
    }
    Ok(())
}

fn looks_like_jwt(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 3 && parts.iter().all(|part| part.len() >= 8)
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
