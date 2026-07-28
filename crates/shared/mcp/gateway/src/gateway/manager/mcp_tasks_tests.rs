use serde_json::json;

use crate::config::GatewayConfig;

use super::*;

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
