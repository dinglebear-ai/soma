//! Dispatch for [`ApiSourceFamily::Official`] capabilities: UniFi's documented
//! `/proxy/network/integration` REST API.

use reqwest::Method;
use serde_json::{json, Value};

use crate::api::{official::OfficialNetworkApi, path, ApiSourceFamily};
use crate::capabilities::Capability;
use crate::error::{Result, UnifiError};
use crate::UnifiClient;

const CONNECTOR_PREFIXES: &[&str] = &["/proxy/network/integration/", "/proxy/protect/integration/"];

/// Runs one official-API `capability` against `client`.
///
/// # Errors
/// Returns [`UnifiError::InvalidRequest`] if `capability` isn't an official-API
/// capability or has no method/path configured; see [`UnifiError`] for the
/// request-level failure cases this can return.
pub async fn execute(
    client: &UnifiClient,
    capability: &Capability,
    params: &Value,
) -> Result<Value> {
    if capability.source != ApiSourceFamily::Official {
        return Err(UnifiError::InvalidRequest {
            context: capability.action.clone(),
            message: "not an official API action".to_string(),
        });
    }
    let path_template = capability
        .path
        .as_deref()
        .ok_or_else(|| UnifiError::InvalidRequest {
            context: capability.action.clone(),
            message: "official action has no path configured".to_string(),
        })?;
    let method = capability
        .method
        .as_deref()
        .unwrap_or("GET")
        .parse::<Method>()
        .map_err(|_| UnifiError::InvalidRequest {
            context: capability.action.clone(),
            message: format!("invalid HTTP method {:?}", capability.method),
        })?;
    let mut effective_params = params.clone();
    normalize_official_request(capability.action.as_str(), &mut effective_params);
    let path = path::substitute_path(path_template, &effective_params, CONNECTOR_PREFIXES)?;
    let api = OfficialNetworkApi::new(&client.url);
    let full_path = api.path(&path);
    client
        .request_json(
            method,
            &full_path,
            effective_params.get("query"),
            effective_params.get("body"),
        )
        .await
}

/// The official "firewall policy ordering" endpoint requires the same zone
/// id as both `sourceFirewallZoneId` and `destinationFirewallZoneId`; fill
/// those in from the single `firewallZoneId` callers pass.
fn normalize_official_request(action: &str, params: &mut Value) {
    if action != "official_get_firewall_policy_ordering" {
        return;
    }
    let Some(zone_id) = params
        .get("query")
        .and_then(|query| query.get("firewallZoneId"))
        .cloned()
    else {
        return;
    };
    if !params.is_object() {
        *params = json!({});
    }
    // `params` is an object as of the line above, so this cannot fail.
    let object = params
        .as_object_mut()
        .expect("params was just made an object");
    let query = object.entry("query").or_insert_with(|| json!({}));
    if !query.is_object() {
        return;
    }
    let query = query
        .as_object_mut()
        .expect("query.is_object() was just checked");
    query
        .entry("sourceFirewallZoneId".to_string())
        .or_insert_with(|| zone_id.clone());
    query
        .entry("destinationFirewallZoneId".to_string())
        .or_insert(zone_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_official_request_fills_both_zone_ids_from_query() {
        let mut params = json!({ "query": { "firewallZoneId": "zone-1" } });

        normalize_official_request("official_get_firewall_policy_ordering", &mut params);

        assert_eq!(
            params,
            json!({
                "query": {
                    "firewallZoneId": "zone-1",
                    "sourceFirewallZoneId": "zone-1",
                    "destinationFirewallZoneId": "zone-1",
                }
            })
        );
    }

    #[test]
    fn normalize_official_request_ignores_other_actions() {
        let mut params = json!({ "query": { "firewallZoneId": "zone-1" } });

        normalize_official_request("official_list_devices", &mut params);

        assert_eq!(params, json!({ "query": { "firewallZoneId": "zone-1" } }));
    }

    #[test]
    fn normalize_official_request_is_a_no_op_without_a_zone_id() {
        let mut params = json!({});

        normalize_official_request("official_get_firewall_policy_ordering", &mut params);

        assert_eq!(params, json!({}));
    }
}
