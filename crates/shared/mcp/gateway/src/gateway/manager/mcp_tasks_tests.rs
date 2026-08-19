use std::{collections::HashMap, time::Duration};

use serde_json::json;

use crate::config::GatewayConfig;
#[cfg(feature = "protected-routes")]
use crate::gateway::protected_routes::ProtectedRouteScope;

use super::*;

#[cfg(feature = "protected-routes")]
fn scope(upstreams: &[&str], services: &[&str]) -> ProtectedRouteScope {
    ProtectedRouteScope {
        upstreams: upstreams.iter().map(|value| (*value).to_owned()).collect(),
        services: services.iter().map(|value| (*value).to_owned()).collect(),
        expose_code_mode: false,
    }
}

#[test]
fn task_ids_are_rewritten_and_bound_to_the_originating_subject() {
    let manager = GatewayManager::new(GatewayConfig::default()).expect("manager");
    let outcome = manager
        .register_task_outcome(
            McpRequestOutcome::Task(json!({
                "resultType": "task",
                "taskId": "native-task",
                "status": "working",
                "createdAt": "2026-07-27T00:00:00Z",
                "lastUpdatedAt": "2026-07-27T00:00:00Z",
                "ttlMs": null
            })),
            "upstream-one",
            Some("alice"),
        )
        .expect("register task");
    let McpRequestOutcome::Task(value) = outcome else {
        panic!("expected task outcome");
    };
    let public_task_id = value["taskId"].as_str().expect("public task id");
    assert!(public_task_id.starts_with("soma-task-"));
    assert_ne!(public_task_id, "native-task");

    let route = manager
        .resolve_task_route(public_task_id, Some("alice"))
        .expect("owner can resolve task");
    assert_eq!(route.upstream, "upstream-one");
    assert_eq!(route.native_task_id, "native-task");
    assert_eq!(route.subject.as_deref(), Some("alice"));
    assert!(matches!(
        manager.resolve_task_route(public_task_id, Some("bob")),
        Err(GatewayManagerError::TaskMissing(_))
    ));
    assert!(matches!(
        manager.resolve_task_route(public_task_id, None),
        Err(GatewayManagerError::TaskMissing(_))
    ));
}

#[cfg(feature = "protected-routes")]
#[test]
fn protected_task_handles_are_bound_to_the_originating_scope() {
    let manager = GatewayManager::new(GatewayConfig::default()).expect("manager");
    let minted_scope = scope(&["upstream-one", "upstream-two"], &["svc-b", "svc-a"]);
    let equivalent_scope = scope(&["upstream-two", "upstream-one"], &["svc-a", "svc-b"]);
    let wrong_scope = scope(&["upstream-one"], &["svc-a", "svc-b"]);
    let outcome = manager
        .register_task_outcome_for_scope(
            McpRequestOutcome::Task(json!({
                "resultType": "task",
                "taskId": "native-task"
            })),
            "upstream-one",
            Some("alice"),
            Some(&minted_scope),
        )
        .expect("register scoped task");
    let McpRequestOutcome::Task(value) = outcome else {
        panic!("expected task outcome");
    };
    let task_id = value["taskId"].as_str().expect("public task id");

    manager
        .resolve_task_route_for_scope(task_id, Some("alice"), Some(&equivalent_scope))
        .expect("equivalent scope can resolve task");
    assert!(matches!(
        manager.resolve_task_route_for_scope(task_id, Some("alice"), None),
        Err(GatewayManagerError::TaskMissing(_))
    ));
    assert!(matches!(
        manager.resolve_task_route_for_scope(task_id, Some("alice"), Some(&wrong_scope)),
        Err(GatewayManagerError::TaskMissing(_))
    ));
}

#[test]
fn task_route_registry_prunes_expired_entries_and_makes_room_by_lru() {
    let now = std::time::Instant::now();
    let mut routes = HashMap::from([
        (
            "expired".to_owned(),
            TaskRoute {
                upstream: "one".to_owned(),
                native_task_id: "expired-native".to_owned(),
                subject: None,
                scope: None,
                last_used: now - TASK_ROUTE_IDLE_TTL - Duration::from_secs(1),
            },
        ),
        (
            "old-live".to_owned(),
            TaskRoute {
                upstream: "one".to_owned(),
                native_task_id: "old-live-native".to_owned(),
                subject: None,
                scope: None,
                last_used: now - Duration::from_secs(20),
            },
        ),
        (
            "new-live".to_owned(),
            TaskRoute {
                upstream: "one".to_owned(),
                native_task_id: "new-live-native".to_owned(),
                subject: None,
                scope: None,
                last_used: now - Duration::from_secs(5),
            },
        ),
    ]);

    prepare_task_routes_for_insert(&mut routes, now, 2);
    assert_eq!(routes.len(), 1, "one slot must remain for the new task");
    assert!(routes.contains_key("new-live"));
    assert!(!routes.contains_key("expired"));
    assert!(!routes.contains_key("old-live"));
}

#[test]
fn invalid_task_results_are_rejected_without_registering_a_route() {
    let manager = GatewayManager::new(GatewayConfig::default()).expect("manager");
    let error = manager
        .register_task_outcome(
            McpRequestOutcome::Task(json!({"resultType": "task"})),
            "upstream-one",
            None,
        )
        .expect_err("missing native task id must fail");
    assert!(matches!(error, GatewayManagerError::InvalidTaskResult(_)));
    assert!(manager.task_routes.read().expect("task routes").is_empty());
}

#[test]
fn gateway_reload_invalidates_public_task_handles() {
    let manager = GatewayManager::new(GatewayConfig::default()).expect("manager");
    let outcome = manager
        .register_task_outcome(
            McpRequestOutcome::Task(json!({
                "resultType": "task",
                "taskId": "native-task"
            })),
            "upstream-one",
            None,
        )
        .expect("register task");
    let McpRequestOutcome::Task(value) = outcome else {
        panic!("expected task outcome");
    };
    let public_task_id = value["taskId"].as_str().expect("public task id").to_owned();

    manager
        .reload_config(GatewayConfig::default())
        .expect("reload gateway");
    assert!(matches!(
        manager.resolve_task_route(&public_task_id, None),
        Err(GatewayManagerError::TaskMissing(_))
    ));
}
