//! Builds [`Capability`] entries from the official-API OpenAPI operation
//! inventory baked into `data/unifi_official_network_v10_3_58.json`.

use serde::Deserialize;

use crate::api::ApiSourceFamily;
use crate::capabilities::{AuthScope, Capability};

#[derive(Debug, Deserialize)]
struct Inventory {
    operations: Vec<Operation>,
}

#[derive(Debug, Deserialize)]
struct Operation {
    method: String,
    path: String,
    operation_id: String,
    summary: String,
}

/// One [`Capability`] per operation in the bundled OpenAPI inventory.
///
/// # Panics
/// Panics if the bundled inventory JSON fails to parse — see
/// [`super::all_capabilities`] for why that can only happen from a broken
/// build, not at runtime.
pub fn capabilities() -> Vec<Capability> {
    let inventory: Inventory = serde_json::from_str(include_str!(
        "../../data/unifi_official_network_v10_3_58.json"
    ))
    .expect("official UniFi inventory should be valid JSON");
    inventory
        .operations
        .into_iter()
        .map(|operation| {
            let mutating = !operation.method.eq_ignore_ascii_case("GET");
            Capability {
                action: action_name(&operation.operation_id),
                title: operation.summary,
                source: ApiSourceFamily::Official,
                method: Some(operation.method),
                path: Some(operation.path),
                mutating,
                auth_scope: if mutating {
                    AuthScope::Admin
                } else {
                    AuthScope::Read
                },
                verification_mode: Some("contract_ok".to_string()),
            }
        })
        .collect()
}

/// Maps an OpenAPI `operationId` to this crate's action-name convention:
/// a curated override for names that read better shortened, otherwise
/// `official_` + the operation id in `snake_case`.
pub fn action_name(operation_id: &str) -> String {
    let override_name = match operation_id {
        "ConnectorDelete" => Some("official_connector_delete"),
        "ConnectorGet" => Some("official_connector_get"),
        "ConnectorPatch" => Some("official_connector_patch"),
        "ConnectorPost" => Some("official_connector_post"),
        "ConnectorPut" => Some("official_connector_put"),
        "getSiteOverviewPage" => Some("official_list_sites"),
        "getConnectedClientOverviewPage" => Some("official_list_clients"),
        "getAdoptedDeviceOverviewPage" => Some("official_list_devices"),
        "getNetworksOverviewPage" => Some("official_list_networks"),
        "getWifiBroadcastPage" => Some("official_list_wifi"),
        _ => None,
    };
    override_name
        .map(str::to_string)
        .unwrap_or_else(|| format!("official_{}", camel_to_snake(operation_id)))
}

fn camel_to_snake(input: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in input.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if idx > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_parses_the_bundled_inventory() {
        let caps = capabilities();

        assert!(!caps.is_empty());
        assert!(caps
            .iter()
            .all(|cap| cap.source == ApiSourceFamily::Official));
    }

    #[test]
    fn action_name_uses_curated_overrides() {
        assert_eq!(action_name("getSiteOverviewPage"), "official_list_sites");
    }

    #[test]
    fn action_name_falls_back_to_snake_case_with_a_prefix() {
        assert_eq!(action_name("getFooBarBaz"), "official_get_foo_bar_baz");
    }

    #[test]
    fn camel_to_snake_does_not_prefix_a_leading_capital_with_an_underscore() {
        assert_eq!(camel_to_snake("Connector"), "connector");
    }

    #[test]
    fn camel_to_snake_splits_on_every_capital() {
        assert_eq!(
            camel_to_snake("getWifiBroadcastPage"),
            "get_wifi_broadcast_page"
        );
    }
}
