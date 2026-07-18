//! Dispatch for [`ApiSourceFamily::Internal`] capabilities: the UniFi
//! controller's own (undocumented, but stable in practice) web-UI API.

use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::Method;
use serde_json::{json, Value};

use crate::api::{internal::InternalNetworkApi, path, ApiSourceFamily};
use crate::capabilities::Capability;
use crate::error::{Result, UnifiError};
use crate::util::truncate_data_array;
use crate::UnifiClient;

/// Runs one internal-API `capability` against `client`.
///
/// Known actions (`clients`, `devices`, `wlans`, `health`, `alarms`,
/// `events`, `sysinfo`, `me`) go through [`UnifiClient`]'s named methods;
/// everything else is dispatched generically from the capability's
/// `method`/`path`.
///
/// # Errors
/// Returns [`UnifiError::InvalidRequest`] if `capability` isn't an
/// internal-API capability; see [`UnifiError`] for the other failure cases
/// this can return.
pub async fn execute(
    client: &UnifiClient,
    capability: &Capability,
    params: &Value,
) -> Result<Value> {
    if capability.source != ApiSourceFamily::Internal {
        return Err(UnifiError::InvalidRequest {
            context: capability.action.clone(),
            message: "not an internal API action".to_string(),
        });
    }

    match capability.action.as_str() {
        "clients" => client.clients().await,
        "devices" => client.devices().await,
        "wlans" => client.wlans().await,
        "health" => client.health().await,
        "alarms" => client.alarms().await,
        "events" => {
            let mut events = execute_generic(client, capability, params).await?;
            truncate_data_array(
                &mut events,
                params
                    .get("limit")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize),
            );
            Ok(events)
        }
        "sysinfo" => client.sysinfo().await,
        "me" => client.me().await,
        _ => execute_generic(client, capability, params).await,
    }
}

async fn execute_generic(
    client: &UnifiClient,
    capability: &Capability,
    params: &Value,
) -> Result<Value> {
    let mut method = capability
        .method
        .as_deref()
        .unwrap_or("GET")
        .parse::<Method>()
        .map_err(|_| UnifiError::InvalidRequest {
            context: capability.action.clone(),
            message: format!("invalid HTTP method {:?}", capability.method),
        })?;
    let mut path = capability
        .path
        .as_deref()
        .ok_or_else(|| UnifiError::InvalidRequest {
            context: capability.action.clone(),
            message: "internal action has no path configured".to_string(),
        })?;
    let mut effective_params = params.clone();
    normalize_internal_request(
        capability.action.as_str(),
        &mut method,
        &mut path,
        &mut effective_params,
    );
    // Substitute against `effective_params`, not the original `params` —
    // `normalize_internal_request` above can rewrite params (it currently
    // only ever injects `body`, never a path parameter, so this was latent
    // rather than live), and path/query/body must all read from the one
    // post-normalization source or a future normalization rule that does
    // touch a path parameter would be silently ignored here.
    let path = path::substitute_path(path, &effective_params, &[])?;
    let api = InternalNetworkApi::new(&client.url, client.site(), client.legacy());
    let full_path = resolve_internal_path(&api, &path, client.legacy());
    let mut value = client
        .request_json(
            method,
            &full_path,
            effective_params.get("query"),
            effective_params.get("body"),
        )
        .await?;
    if capability.action == "unifi_get_ips_events" {
        retain_security_events(&mut value);
    }
    Ok(value)
}

/// Internal-API paths come in three shapes that each map onto the controller
/// URL differently: the fixed `/api/self` endpoint, other legacy `/api/...`
/// endpoints, and the `/v2/...` endpoints that live under a per-site prefix
/// supplied by `api` (never by the capability's own path template — none of
/// the bundled inventory's `/v2/...` paths embed a site segment themselves).
fn resolve_internal_path(api: &InternalNetworkApi, path: &str, legacy: bool) -> String {
    if path == "/api/self" {
        if legacy {
            path.to_string()
        } else {
            "/proxy/network/api/self".to_string()
        }
    } else if path.starts_with("/api/") {
        if legacy {
            path.to_string()
        } else {
            format!("/proxy/network{path}")
        }
    } else if let Some(suffix) = path.strip_prefix("/v2/") {
        api.v2_site_path(suffix)
    } else {
        api.v1_site_path(path)
    }
}

/// A handful of internal actions don't map 1:1 onto their capability's
/// declared method/path/body — the controller's web UI issues them as `POST`
/// with a JSON body even though the capability catalog (built for
/// discoverability) lists them as simple lookups. Patch the request here
/// rather than special-casing the catalog.
fn normalize_internal_request(
    action: &str,
    method: &mut Method,
    path: &mut &str,
    params: &mut Value,
) {
    match action {
        "events" | "unifi_recent_events" | "unifi_get_ips_events" => {
            *method = Method::POST;
            *path = "/v2/system-log/all";
            ensure_body(params, json!({}));
        }
        "unifi_get_client_sessions" => {
            ensure_body(params, default_session_body());
        }
        "unifi_get_alerts"
        | "unifi_get_event_types"
        | "unifi_get_traffic_flows"
        | "unifi_list_alarms"
        | "unifi_list_events" => {
            ensure_body(params, json!({}));
        }
        _ => {}
    }
}

fn ensure_body(params: &mut Value, body: Value) {
    if params.get("body").is_some() {
        return;
    }
    if !params.is_object() {
        *params = json!({});
    }
    if let Some(object) = params.as_object_mut() {
        object.insert("body".to_string(), body);
    }
}

/// A 24-hour window ending now, in epoch milliseconds — the default range
/// the controller's client-session endpoint expects when the caller didn't
/// supply one.
fn default_session_body() -> Value {
    let end = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    let start = end.saturating_sub(24 * 60 * 60 * 1000);
    json!({ "start": start, "end": end })
}

/// `unifi_get_ips_events` shares its endpoint with the general system-log
/// feed; keep only entries that are actually IPS/threat security events.
fn retain_security_events(value: &mut Value) {
    let Some(items) = value.get_mut("data").and_then(Value::as_array_mut) else {
        return;
    };
    items.retain(|item| {
        matches!(
            item.get("category").and_then(Value::as_str),
            Some("SECURITY")
        ) || item
            .get("key")
            .and_then(Value::as_str)
            .is_some_and(|key| key.contains("THREAT") || key.contains("IPS"))
            || item
                .get("subcategory")
                .and_then(Value::as_str)
                .is_some_and(|subcategory| subcategory.contains("SECURITY"))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_internal_path_maps_self_endpoint() {
        let api = InternalNetworkApi::new("https://unifi.local", "default", false);

        assert_eq!(
            resolve_internal_path(&api, "/api/self", false),
            "/proxy/network/api/self"
        );
        assert_eq!(resolve_internal_path(&api, "/api/self", true), "/api/self");
    }

    #[test]
    fn resolve_internal_path_maps_legacy_api_endpoints() {
        let api = InternalNetworkApi::new("https://unifi.local", "default", false);

        assert_eq!(
            resolve_internal_path(&api, "/api/stat/sta", false),
            "/proxy/network/api/stat/sta"
        );
        assert_eq!(
            resolve_internal_path(&api, "/api/stat/sta", true),
            "/api/stat/sta"
        );
    }

    #[test]
    fn resolve_internal_path_maps_v2_endpoints_through_the_site_prefix() {
        let api = InternalNetworkApi::new("https://unifi.local", "default", false);

        assert_eq!(
            resolve_internal_path(&api, "/v2/system-log/all", false),
            "/proxy/network/v2/api/site/default/system-log/all"
        );
    }

    #[test]
    fn resolve_internal_path_falls_back_to_v1_site_path() {
        let api = InternalNetworkApi::new("https://unifi.local", "default", false);

        assert_eq!(
            resolve_internal_path(&api, "stat/sta", false),
            "/proxy/network/api/s/default/stat/sta"
        );
    }

    #[test]
    fn normalize_internal_request_rewrites_events_to_a_post() {
        let mut method = Method::GET;
        let mut path = "/rest/event";
        let mut params = Value::Null;

        normalize_internal_request("events", &mut method, &mut path, &mut params);

        assert_eq!(method, Method::POST);
        assert_eq!(path, "/v2/system-log/all");
        assert_eq!(params.get("body"), Some(&json!({})));
    }

    #[test]
    fn normalize_internal_request_preserves_a_caller_supplied_body() {
        let mut method = Method::GET;
        let mut path = "/rest/event";
        let mut params = json!({ "body": { "start": 1 } });

        normalize_internal_request("events", &mut method, &mut path, &mut params);

        assert_eq!(params.get("body"), Some(&json!({ "start": 1 })));
    }

    #[test]
    fn normalize_internal_request_leaves_unmapped_actions_alone() {
        let mut method = Method::GET;
        let mut path = "/stat/sta";
        let mut params = Value::Null;

        normalize_internal_request("clients", &mut method, &mut path, &mut params);

        assert_eq!(method, Method::GET);
        assert_eq!(path, "/stat/sta");
        assert_eq!(params, Value::Null);
    }

    #[test]
    fn normalize_internal_request_does_not_reroute_traffic_flow_statistics_to_the_flows_listing() {
        // Regression test: this used to overwrite the inventory-declared
        // `GET /v2/traffic-flow-latest-statistics` with the unrelated
        // `POST /v2/traffic-flows` (the same rewrite `unifi_get_traffic_flows`
        // gets), silently returning the wrong data.
        let mut method = Method::GET;
        let mut path = "/v2/traffic-flow-latest-statistics";
        let mut params = Value::Null;

        normalize_internal_request(
            "unifi_get_traffic_flow_statistics",
            &mut method,
            &mut path,
            &mut params,
        );

        assert_eq!(method, Method::GET);
        assert_eq!(path, "/v2/traffic-flow-latest-statistics");
    }

    #[test]
    fn normalize_internal_request_does_not_reroute_gateway_settings_to_mgmt_settings() {
        // Regression test: this used to overwrite the inventory-declared
        // `/get/setting/gateway` with `/get/setting/mgmt`, a different
        // settings object.
        let mut method = Method::GET;
        let mut path = "/get/setting/gateway";
        let mut params = Value::Null;

        normalize_internal_request(
            "unifi_get_gateway_settings",
            &mut method,
            &mut path,
            &mut params,
        );

        assert_eq!(path, "/get/setting/gateway");
    }

    #[test]
    fn default_session_body_covers_a_24_hour_window() {
        let body = default_session_body();

        let start = body["start"].as_u64().unwrap();
        let end = body["end"].as_u64().unwrap();

        assert_eq!(end - start, 24 * 60 * 60 * 1000);
    }

    #[test]
    fn retain_security_events_keeps_only_security_flagged_items() {
        let mut value = json!({
            "data": [
                { "category": "SECURITY", "key": "x" },
                { "category": "OTHER", "key": "THREAT_DETECTED" },
                { "category": "OTHER", "subcategory": "SECURITY_ALERT" },
                { "category": "OTHER", "key": "noise" },
            ]
        });

        retain_security_events(&mut value);

        assert_eq!(value["data"].as_array().unwrap().len(), 3);
    }
}
