//! Capability brokering: the [`CapabilityBroker`] decides which host
//! capabilities a provider dispatch may use, defaulting to deny.
use soma_provider_core::{
    BrokerCapability, BrowserCapability, CapabilityGrant, EnvCapability, FilesystemCapability,
    GitHubCapability, HostCapabilities, NetworkCapability, TerminalCapability,
};

use crate::provider_errors::ProviderError;

/// Checks provider-requested host capabilities against a fixed set of policy
/// grants, denying any capability that is not explicitly allowed.
#[derive(Debug, Clone, Default)]
pub struct CapabilityBroker {
    grants: Vec<(String, CapabilityGrant)>,
}

impl CapabilityBroker {
    /// Builds a broker backed by the given policy grants.
    pub fn new(grants: Vec<(String, CapabilityGrant)>) -> Self {
        Self { grants }
    }

    /// Builds a broker with no grants, denying every host capability.
    pub fn default_deny() -> Self {
        Self::default()
    }

    /// Authorizes a provider action's requested host capabilities, returning
    /// an error for the first requested capability no grant covers.
    pub fn authorize(
        &self,
        provider: &str,
        action: &str,
        requested: &HostCapabilities,
    ) -> Result<(), ProviderError> {
        if let Some(capability) = enabled_filesystem(requested) {
            let granted = self.grants.iter().any(|(owner, grant)| {
                owner == provider
                    && match grant {
                        CapabilityGrant::Filesystem {
                            read_roots,
                            write_roots,
                        } => {
                            all_paths_allowed(&capability.read_roots, read_roots)
                                && all_paths_allowed(&capability.write_roots, write_roots)
                        }
                        _ => false,
                    }
            });
            if !granted {
                return Err(denied(provider, action, "filesystem"));
            }
        }
        if let Some(capability) = enabled_network(requested) {
            let granted = self.grants.iter().any(|(owner, grant)| {
                owner == provider
                    && match grant {
                        CapabilityGrant::Network { allowed_hosts } => {
                            all_items_allowed(&capability.allowed_hosts, allowed_hosts)
                        }
                        _ => false,
                    }
            });
            if !granted {
                return Err(denied(provider, action, "network"));
            }
        }
        if let Some(capability) = enabled_env(requested) {
            let granted = self.grants.iter().any(|(owner, grant)| {
                owner == provider
                    && match grant {
                        CapabilityGrant::Env { allowed } => {
                            all_items_allowed(&capability.allowed, allowed)
                        }
                        _ => false,
                    }
            });
            if !granted {
                return Err(denied(provider, action, "env"));
            }
        }
        if let Some(capability) = enabled_terminal(requested) {
            let granted = self.grants.iter().any(|(owner, grant)| {
                owner == provider
                    && match grant {
                        CapabilityGrant::Terminal {
                            working_dir,
                            allowlist,
                        } => {
                            working_dir_allows(
                                capability.working_dir.as_deref(),
                                working_dir.as_deref(),
                            ) && all_items_allowed(&capability.allowlist, allowlist)
                        }
                        _ => false,
                    }
            });
            if !granted {
                return Err(denied(provider, action, "terminal"));
            }
        }
        if let Some(capability) = enabled_browser(requested) {
            let granted = self.grants.iter().any(|(owner, grant)| {
                owner == provider
                    && match grant {
                        CapabilityGrant::Browser { allowed_origins } => {
                            all_items_allowed(&capability.allowed_origins, allowed_origins)
                        }
                        _ => false,
                    }
            });
            if !granted {
                return Err(denied(provider, action, "browser"));
            }
        }
        if let Some(capability) = enabled_github(requested) {
            let granted = self.grants.iter().any(|(owner, grant)| {
                owner == provider
                    && match grant {
                        CapabilityGrant::Github {
                            allowed_repos,
                            read_only,
                        } => {
                            all_items_allowed(&capability.allowed_repos, allowed_repos)
                                && (capability.read_only || !read_only)
                        }
                        _ => false,
                    }
            });
            if !granted {
                return Err(denied(provider, action, "github"));
            }
        }
        if let Some(capability) = enabled_broker(requested) {
            let provider_scoped = capability
                .secret_names
                .iter()
                .all(|name| provider_owns_name(provider, name))
                && capability
                    .state_namespace
                    .as_ref()
                    .is_none_or(|name| provider_owns_name(provider, name));
            if !provider_scoped {
                return Err(denied(provider, action, "broker"));
            }
            let granted = self.grants.iter().any(|(owner, grant)| {
                owner == provider
                    && match grant {
                        CapabilityGrant::Broker {
                            secret_names,
                            state_read_namespaces,
                            state_write_namespaces,
                            allow_logging,
                            allow_metrics,
                            allow_progress,
                        } => {
                            all_items_allowed(&capability.secret_names, secret_names)
                                && capability.state_namespace.as_ref().is_none_or(|namespace| {
                                    state_read_namespaces.contains(namespace)
                                        && (!capability.state_write
                                            || state_write_namespaces.contains(namespace))
                                })
                                && (!capability.logging || *allow_logging)
                                && (!capability.metrics || *allow_metrics)
                                && (!capability.progress || *allow_progress)
                        }
                        _ => false,
                    }
            });
            if !granted {
                return Err(denied(provider, action, "broker"));
            }
        }
        Ok(())
    }
}

fn provider_owns_name(provider: &str, name: &str) -> bool {
    name == provider
        || name
            .strip_prefix(provider)
            .is_some_and(|suffix| suffix.starts_with('-') || suffix.starts_with('/'))
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

fn enabled_broker(requested: &HostCapabilities) -> Option<&BrokerCapability> {
    requested
        .broker
        .as_ref()
        .filter(|capability| capability.enabled)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broker_grants_intersect_every_requested_authority() {
        let broker = CapabilityBroker::new(vec![(
            "weather".to_owned(),
            CapabilityGrant::Broker {
                secret_names: vec!["weather-key".to_owned()],
                state_read_namespaces: vec!["weather".to_owned()],
                state_write_namespaces: vec!["weather".to_owned()],
                allow_logging: true,
                allow_metrics: false,
                allow_progress: true,
            },
        )]);
        let requested = HostCapabilities {
            broker: Some(BrokerCapability {
                enabled: true,
                secret_names: vec!["weather-key".to_owned()],
                state_namespace: Some("weather".to_owned()),
                state_write: true,
                logging: true,
                metrics: false,
                progress: true,
            }),
            ..HostCapabilities::default()
        };
        broker
            .authorize("weather", "forecast", &requested)
            .expect("matching provider and deployment authority is allowed");
        assert!(
            broker.authorize("billing", "forecast", &requested).is_err(),
            "a deployment grant for one provider must not become global authority"
        );

        let mut excessive = requested.clone();
        excessive.broker.as_mut().unwrap().metrics = true;
        let error = broker
            .authorize("weather", "forecast", &excessive)
            .expect_err("an ungranted metric request must be denied");
        assert_eq!(&*error.code, "capability_denied");

        let read_only = CapabilityBroker::new(vec![(
            "weather".to_owned(),
            CapabilityGrant::Broker {
                secret_names: Vec::new(),
                state_read_namespaces: vec!["weather".to_owned()],
                state_write_namespaces: Vec::new(),
                allow_logging: false,
                allow_metrics: false,
                allow_progress: false,
            },
        )]);
        let error = read_only
            .authorize("weather", "forecast", &requested)
            .expect_err("read-only deployment policy must deny state writes");
        assert_eq!(&*error.code, "capability_denied");
    }
}
