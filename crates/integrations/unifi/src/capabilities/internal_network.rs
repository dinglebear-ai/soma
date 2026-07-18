//! Builds [`Capability`] entries from the internal-API endpoint inventory
//! baked into `data/unifi_internal_endpoint_models.json`, plus a handful of
//! hand-written legacy and hybrid aliases.

use serde::Deserialize;

use crate::api::ApiSourceFamily;
use crate::capabilities::{AuthScope, Capability};

#[derive(Debug, Deserialize)]
struct Inventory {
    tools: Vec<Tool>,
}

#[derive(Debug, Deserialize)]
struct Tool {
    action: String,
    method: String,
    path: String,
    title: String,
    mutating: bool,
    runtime: bool,
    auth_scope: String,
    verification_mode: String,
}

/// One [`Capability`] per `runtime`-flagged tool in the bundled endpoint
/// inventory, plus the fixed legacy aliases ([`UnifiClient`](crate::UnifiClient)'s
/// named methods) and hybrid entries this crate resolves at dispatch time.
///
/// # Panics
/// Panics if the bundled inventory JSON fails to parse, or if it contains an
/// `auth_scope` other than `"read"`/`"admin"` — see [`super::all_capabilities`]
/// for why that can only happen from a broken build, not at runtime.
pub fn capabilities() -> Vec<Capability> {
    let inventory: Inventory = serde_json::from_str(include_str!(
        "../../data/unifi_internal_endpoint_models.json"
    ))
    .expect("internal UniFi endpoint models should be valid JSON");
    let mut caps = inventory
        .tools
        .into_iter()
        .filter(|tool| tool.runtime)
        .map(|tool| Capability {
            action: tool.action,
            title: tool.title,
            source: ApiSourceFamily::Internal,
            method: Some(tool.method),
            path: Some(tool.path),
            mutating: tool.mutating,
            auth_scope: auth_scope(&tool.auth_scope),
            verification_mode: Some(tool.verification_mode),
        })
        .collect::<Vec<_>>();
    caps.extend([
        legacy("clients", "Clients", "GET", "/stat/sta"),
        legacy("devices", "Devices", "GET", "/stat/device"),
        legacy("wlans", "WLANs", "GET", "/rest/wlanconf"),
        legacy("health", "Health", "GET", "/stat/health"),
        // Matches UnifiClient::alarms()'s actual call in client.rs — a
        // catalog/{legacy alias} path mismatch here was found in review;
        // "alarms" is dispatched through the named handler in
        // actions/internal.rs, so this path is discovery metadata only,
        // but it must describe what will really be called.
        legacy("alarms", "Alarms", "GET", "/rest/alarm"),
        legacy("events", "Events", "GET", "/rest/event"),
        legacy("sysinfo", "System Info", "GET", "/stat/sysinfo"),
        legacy("me", "Current User", "GET", "/api/self"),
        hybrid("list_clients", "List Clients"),
        hybrid("list_devices", "List Devices"),
        hybrid("list_networks", "List Networks"),
        hybrid("list_wifi", "List WiFi"),
        hybrid("get_system_info", "Get System Info"),
    ]);
    caps
}

fn legacy(action: &str, title: &str, method: &str, path: &str) -> Capability {
    Capability {
        action: action.to_string(),
        title: title.to_string(),
        source: ApiSourceFamily::Internal,
        method: Some(method.to_string()),
        path: Some(path.to_string()),
        mutating: false,
        auth_scope: AuthScope::Read,
        verification_mode: Some("legacy_alias".to_string()),
    }
}

fn hybrid(action: &str, title: &str) -> Capability {
    Capability {
        action: action.to_string(),
        title: title.to_string(),
        source: ApiSourceFamily::Hybrid,
        method: None,
        path: None,
        mutating: false,
        auth_scope: AuthScope::Read,
        verification_mode: Some("contract_ok".to_string()),
    }
}

fn auth_scope(scope: &str) -> AuthScope {
    match scope {
        "read" => AuthScope::Read,
        "admin" => AuthScope::Admin,
        other => panic!("unknown internal auth_scope {other} in bundled inventory JSON"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_parses_the_bundled_inventory_and_appends_aliases() {
        let caps = capabilities();

        assert!(caps
            .iter()
            .any(|cap| cap.action == "clients" && cap.source == ApiSourceFamily::Internal));
        assert!(caps
            .iter()
            .any(|cap| cap.action == "list_clients" && cap.source == ApiSourceFamily::Hybrid));
    }

    #[test]
    fn unifi_block_client_is_excluded_until_its_real_endpoint_is_verified() {
        // The bundled inventory declares this mutating admin action with the
        // same GET path as the read-only client listing — dispatching it
        // would silently no-op instead of blocking anything. It must stay
        // `runtime: false` (and therefore absent here) until fixed.
        let caps = capabilities();

        assert!(!caps.iter().any(|cap| cap.action == "unifi_block_client"));
    }

    #[test]
    fn no_dispatchable_mutating_action_shares_a_get_path_with_a_read_only_action() {
        // A mutating admin action declared as a GET against the exact same
        // path a read-only action already uses cannot actually mutate
        // anything: dispatching it just re-runs the read and returns a
        // misleadingly successful result (this is precisely the bug
        // `unifi_block_client`, and 20 siblings alongside it, shipped with —
        // see the `evidence` field on any `unverified_path_mismatch` entry
        // in data/unifi_internal_endpoint_models.json for the fix history).
        // This is a catalog-wide invariant, not a one-action regression pin:
        // it catches the whole bug class, including future additions.
        let caps = capabilities();
        let read_only_paths: std::collections::HashSet<&str> = caps
            .iter()
            .filter(|cap| !cap.mutating)
            .filter_map(|cap| cap.path.as_deref())
            .collect();

        let offenders: Vec<&str> = caps
            .iter()
            .filter(|cap| cap.mutating)
            .filter(|cap| {
                cap.method
                    .as_deref()
                    .is_some_and(|m| m.eq_ignore_ascii_case("GET"))
                    && cap
                        .path
                        .as_deref()
                        .is_some_and(|p| read_only_paths.contains(p))
            })
            .map(|cap| cap.action.as_str())
            .collect();

        assert!(
            offenders.is_empty(),
            "mutating actions with a GET path identical to a read-only action's path \
             (cannot actually mutate anything; disable via runtime:false in the JSON \
             inventory until the real endpoint is confirmed): {offenders:?}"
        );
    }

    #[test]
    fn legacy_aliases_are_read_scoped_and_non_mutating() {
        let cap = legacy("clients", "Clients", "GET", "/stat/sta");

        assert_eq!(cap.auth_scope, AuthScope::Read);
        assert!(!cap.mutating);
    }

    #[test]
    fn hybrid_entries_have_no_method_or_path() {
        let cap = hybrid("list_clients", "List Clients");

        assert_eq!(cap.source, ApiSourceFamily::Hybrid);
        assert!(cap.method.is_none());
        assert!(cap.path.is_none());
    }

    #[test]
    #[should_panic(expected = "unknown internal auth_scope")]
    fn auth_scope_panics_on_an_unrecognized_value() {
        auth_scope("write");
    }
}
