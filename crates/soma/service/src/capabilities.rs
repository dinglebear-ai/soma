use soma_contracts::providers::{
    BrowserCapability, CapabilityGrant, EnvCapability, FilesystemCapability, GitHubCapability,
    HostCapabilities, NetworkCapability, TerminalCapability,
};

use crate::provider_errors::ProviderError;

#[derive(Debug, Clone, Default)]
pub struct CapabilityBroker {
    grants: Vec<CapabilityGrant>,
}

impl CapabilityBroker {
    pub fn new(grants: Vec<CapabilityGrant>) -> Self {
        Self { grants }
    }

    pub fn default_deny() -> Self {
        Self::default()
    }

    pub fn authorize(
        &self,
        provider: &str,
        action: &str,
        requested: &HostCapabilities,
    ) -> Result<(), ProviderError> {
        if let Some(capability) = enabled_filesystem(requested) {
            let granted = self.grants.iter().any(|grant| match grant {
                CapabilityGrant::Filesystem {
                    read_roots,
                    write_roots,
                } => {
                    all_paths_allowed(&capability.read_roots, read_roots)
                        && all_paths_allowed(&capability.write_roots, write_roots)
                }
                _ => false,
            });
            if !granted {
                return Err(denied(provider, action, "filesystem"));
            }
        }
        if let Some(capability) = enabled_network(requested) {
            let granted = self.grants.iter().any(|grant| match grant {
                CapabilityGrant::Network { allowed_hosts } => {
                    all_items_allowed(&capability.allowed_hosts, allowed_hosts)
                }
                _ => false,
            });
            if !granted {
                return Err(denied(provider, action, "network"));
            }
        }
        if let Some(capability) = enabled_env(requested) {
            let granted = self.grants.iter().any(|grant| match grant {
                CapabilityGrant::Env { allowed } => all_items_allowed(&capability.allowed, allowed),
                _ => false,
            });
            if !granted {
                return Err(denied(provider, action, "env"));
            }
        }
        if let Some(capability) = enabled_terminal(requested) {
            let granted = self.grants.iter().any(|grant| match grant {
                CapabilityGrant::Terminal {
                    working_dir,
                    allowlist,
                } => {
                    working_dir_allows(capability.working_dir.as_deref(), working_dir.as_deref())
                        && all_items_allowed(&capability.allowlist, allowlist)
                }
                _ => false,
            });
            if !granted {
                return Err(denied(provider, action, "terminal"));
            }
        }
        if let Some(capability) = enabled_browser(requested) {
            let granted = self.grants.iter().any(|grant| match grant {
                CapabilityGrant::Browser { allowed_origins } => {
                    all_items_allowed(&capability.allowed_origins, allowed_origins)
                }
                _ => false,
            });
            if !granted {
                return Err(denied(provider, action, "browser"));
            }
        }
        if let Some(capability) = enabled_github(requested) {
            let granted = self.grants.iter().any(|grant| match grant {
                CapabilityGrant::Github {
                    allowed_repos,
                    read_only,
                } => {
                    all_items_allowed(&capability.allowed_repos, allowed_repos)
                        && (capability.read_only || !read_only)
                }
                _ => false,
            });
            if !granted {
                return Err(denied(provider, action, "github"));
            }
        }
        Ok(())
    }
}

fn enabled_filesystem(requested: &HostCapabilities) -> Option<&FilesystemCapability> {
    requested.filesystem.as_ref().filter(|cap| cap.enabled)
}

fn enabled_network(requested: &HostCapabilities) -> Option<&NetworkCapability> {
    requested.network.as_ref().filter(|cap| cap.enabled)
}

fn enabled_env(requested: &HostCapabilities) -> Option<&EnvCapability> {
    requested.env.as_ref().filter(|cap| cap.enabled)
}

fn enabled_terminal(requested: &HostCapabilities) -> Option<&TerminalCapability> {
    requested.terminal.as_ref().filter(|cap| cap.enabled)
}

fn enabled_browser(requested: &HostCapabilities) -> Option<&BrowserCapability> {
    requested.browser.as_ref().filter(|cap| cap.enabled)
}

fn enabled_github(requested: &HostCapabilities) -> Option<&GitHubCapability> {
    requested.github.as_ref().filter(|cap| cap.enabled)
}

fn all_items_allowed(requested: &[String], allowed: &[String]) -> bool {
    requested
        .iter()
        .all(|item| allowed.iter().any(|grant| grant == item))
}

fn all_paths_allowed(requested: &[String], allowed: &[String]) -> bool {
    requested
        .iter()
        .all(|item| allowed.iter().any(|grant| path_allows(item, grant)))
}

fn path_allows(requested: &str, grant: &str) -> bool {
    requested == grant
        || requested
            .strip_prefix(grant.trim_end_matches('/'))
            .map(|suffix| suffix.starts_with('/'))
            .unwrap_or(false)
}

fn working_dir_allows(requested: Option<&str>, granted: Option<&str>) -> bool {
    match (requested, granted) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(requested), Some(granted)) => path_allows(requested, granted),
    }
}

fn denied(provider: &str, action: &str, capability: &str) -> ProviderError {
    ProviderError::new(
        "capability_denied",
        provider,
        Some(action.to_owned()),
        format!("provider requested denied {capability} capability"),
        "Grant the specific host capability in policy or disable the provider action.",
    )
}
