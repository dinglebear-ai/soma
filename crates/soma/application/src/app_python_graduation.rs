use soma_provider_core::HostCapabilities;

pub(super) fn side_effecting_capabilities(capabilities: &HostCapabilities) -> bool {
    capabilities
        .filesystem
        .as_ref()
        .is_some_and(|capability| capability.enabled)
        || capabilities
            .network
            .as_ref()
            .is_some_and(|capability| capability.enabled)
        || capabilities
            .env
            .as_ref()
            .is_some_and(|capability| capability.enabled)
        || capabilities
            .terminal
            .as_ref()
            .is_some_and(|capability| capability.enabled)
        || capabilities
            .browser
            .as_ref()
            .is_some_and(|capability| capability.enabled)
        || capabilities
            .github
            .as_ref()
            .is_some_and(|capability| capability.enabled)
        || capabilities
            .broker
            .as_ref()
            .is_some_and(|broker| broker.state_write || !broker.secret_names.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soma_provider_core::{BrokerCapability, NetworkCapability};

    #[test]
    fn dual_run_rejects_external_and_persistent_authority() {
        assert!(!side_effecting_capabilities(&HostCapabilities::default()));
        assert!(side_effecting_capabilities(&HostCapabilities {
            network: Some(NetworkCapability {
                enabled: true,
                ..NetworkCapability::default()
            }),
            ..HostCapabilities::default()
        }));
        assert!(side_effecting_capabilities(&HostCapabilities {
            broker: Some(BrokerCapability {
                enabled: true,
                state_write: true,
                ..BrokerCapability::default()
            }),
            ..HostCapabilities::default()
        }));
    }
}
