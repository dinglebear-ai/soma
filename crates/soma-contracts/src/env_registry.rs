//! Canonical environment-variable registry.
//!
//! Keep setup, docs, and plugin option mapping pointed at this table instead of
//! scattering env-var knowledge across shell snippets and manifests.

#[cfg(test)]
#[path = "env_registry_tests.rs"]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvClassification {
    KeepEnv,
    ComposeEnv,
    TrustedOperatorBootstrap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePlacement {
    HostOnly,
    ContainerRequired,
    ComposeInterpolation,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyBehavior {
    Canonical,
    WarnEnvOverride,
    Advanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvKeySpec {
    pub key: &'static str,
    pub classification: EnvClassification,
    pub placement: RuntimePlacement,
    pub toml_destination: Option<&'static str>,
    pub legacy_behavior: LegacyBehavior,
    pub secret: bool,
    pub plugin_option: Option<&'static str>,
}

pub const fn spec(
    key: &'static str,
    classification: EnvClassification,
    placement: RuntimePlacement,
    toml_destination: Option<&'static str>,
    legacy_behavior: LegacyBehavior,
    secret: bool,
    plugin_option: Option<&'static str>,
) -> EnvKeySpec {
    EnvKeySpec {
        key,
        classification,
        placement,
        toml_destination,
        legacy_behavior,
        secret,
        plugin_option,
    }
}

pub const ENV_KEY_SPECS: &[EnvKeySpec] = &[
    spec(
        "SOMA_API_URL",
        EnvClassification::KeepEnv,
        RuntimePlacement::HostOnly,
        Some("soma.api_url"),
        LegacyBehavior::Canonical,
        false,
        Some("CLAUDE_PLUGIN_OPTION_SOMA_API_URL"),
    ),
    spec(
        "SOMA_API_KEY",
        EnvClassification::KeepEnv,
        RuntimePlacement::HostOnly,
        Some("soma.api_key"),
        LegacyBehavior::Canonical,
        true,
        Some("CLAUDE_PLUGIN_OPTION_SOMA_API_KEY"),
    ),
    spec(
        "SOMA_MCP_TOKEN",
        EnvClassification::TrustedOperatorBootstrap,
        RuntimePlacement::Both,
        Some("mcp.api_token"),
        LegacyBehavior::Canonical,
        true,
        Some("CLAUDE_PLUGIN_OPTION_API_TOKEN"),
    ),
    spec(
        "SOMA_SERVER_URL",
        EnvClassification::KeepEnv,
        RuntimePlacement::HostOnly,
        None,
        LegacyBehavior::WarnEnvOverride,
        false,
        Some("CLAUDE_PLUGIN_OPTION_SERVER_URL"),
    ),
    spec(
        "SOMA_MCP_AUTH_MODE",
        EnvClassification::KeepEnv,
        RuntimePlacement::Both,
        Some("mcp.auth.mode"),
        LegacyBehavior::Canonical,
        false,
        Some("CLAUDE_PLUGIN_OPTION_AUTH_MODE"),
    ),
    spec(
        "SOMA_MCP_NO_AUTH",
        EnvClassification::TrustedOperatorBootstrap,
        RuntimePlacement::HostOnly,
        Some("mcp.no_auth"),
        LegacyBehavior::Advanced,
        false,
        Some("CLAUDE_PLUGIN_OPTION_NO_AUTH"),
    ),
    spec(
        "SOMA_NOAUTH",
        EnvClassification::TrustedOperatorBootstrap,
        RuntimePlacement::Both,
        Some("mcp.trusted_gateway"),
        LegacyBehavior::Advanced,
        false,
        None,
    ),
    spec(
        "SOMA_MCP_PUBLIC_URL",
        EnvClassification::KeepEnv,
        RuntimePlacement::Both,
        Some("mcp.auth.public_url"),
        LegacyBehavior::Canonical,
        false,
        Some("CLAUDE_PLUGIN_OPTION_PUBLIC_URL"),
    ),
    spec(
        "SOMA_MCP_GOOGLE_CLIENT_ID",
        EnvClassification::TrustedOperatorBootstrap,
        RuntimePlacement::Both,
        Some("mcp.auth.google_client_id"),
        LegacyBehavior::Canonical,
        true,
        Some("CLAUDE_PLUGIN_OPTION_GOOGLE_CLIENT_ID"),
    ),
    spec(
        "SOMA_MCP_GOOGLE_CLIENT_SECRET",
        EnvClassification::TrustedOperatorBootstrap,
        RuntimePlacement::Both,
        Some("mcp.auth.google_client_secret"),
        LegacyBehavior::Canonical,
        true,
        Some("CLAUDE_PLUGIN_OPTION_GOOGLE_CLIENT_SECRET"),
    ),
    spec(
        "SOMA_MCP_AUTH_ADMIN_EMAIL",
        EnvClassification::TrustedOperatorBootstrap,
        RuntimePlacement::Both,
        Some("mcp.auth.admin_email"),
        LegacyBehavior::Canonical,
        false,
        Some("CLAUDE_PLUGIN_OPTION_AUTH_ADMIN_EMAIL"),
    ),
    spec(
        "SOMA_MCP_HOST",
        EnvClassification::ComposeEnv,
        RuntimePlacement::Both,
        Some("mcp.host"),
        LegacyBehavior::Canonical,
        false,
        None,
    ),
    spec(
        "SOMA_MCP_SERVER_NAME",
        EnvClassification::ComposeEnv,
        RuntimePlacement::Both,
        Some("mcp.server_name"),
        LegacyBehavior::Canonical,
        false,
        None,
    ),
    spec(
        "SOMA_MCP_PORT",
        EnvClassification::ComposeEnv,
        RuntimePlacement::Both,
        Some("mcp.port"),
        LegacyBehavior::Canonical,
        false,
        None,
    ),
    spec(
        "SOMA_MCP_ALLOWED_HOSTS",
        EnvClassification::ComposeEnv,
        RuntimePlacement::Both,
        Some("mcp.allowed_hosts"),
        LegacyBehavior::Canonical,
        false,
        None,
    ),
    spec(
        "SOMA_MCP_ALLOWED_ORIGINS",
        EnvClassification::ComposeEnv,
        RuntimePlacement::Both,
        Some("mcp.allowed_origins"),
        LegacyBehavior::Canonical,
        false,
        None,
    ),
];

pub fn all_specs() -> &'static [EnvKeySpec] {
    ENV_KEY_SPECS
}

pub fn spec_for(key: &str) -> Option<&'static EnvKeySpec> {
    ENV_KEY_SPECS.iter().find(|spec| spec.key == key)
}

pub fn plugin_option_mappings() -> impl Iterator<Item = (&'static str, &'static str)> {
    ENV_KEY_SPECS
        .iter()
        .filter_map(|spec| spec.plugin_option.map(|option| (option, spec.key)))
}
