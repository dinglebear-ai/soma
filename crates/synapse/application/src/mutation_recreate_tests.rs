use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde_json::json;
use soma_infra::ContainerState;
use soma_ops::{OperationName, OperationStatus};
use tokio_util::sync::CancellationToken;

use crate::mutation_pull_test_support::{authorization, context};
use crate::mutation_recreate_test_support::{
    FakeComposeRecreate, FakeContainerRecreate, compose_config, compose_status, container_inspect,
    fingerprint, projects, runtime,
};
use crate::{ExecutionError, SynapseMutationRuntime};

#[tokio::test]
async fn container_recreate_binds_prestate_executes_and_verifies() {
    let expected = fingerprint("a");
    let container = Arc::new(FakeContainerRecreate {
        fingerprints: Mutex::new(VecDeque::from([
            expected.clone(),
            expected.clone(),
            expected.clone(),
        ])),
        inspections: Mutex::new(VecDeque::from([
            container_inspect("old-id", ContainerState::Running),
            container_inspect("new-id", ContainerState::Running),
        ])),
        mutations: Mutex::new(0),
    });
    let compose = Arc::new(FakeComposeRecreate {
        projects: Mutex::new(VecDeque::new()),
        configs: Mutex::new(VecDeque::new()),
        statuses: Mutex::new(VecDeque::new()),
        mutations: Mutex::new(0),
    });
    let runtime = runtime(Arc::clone(&container), compose);
    let operation = OperationName::new("container.recreate").unwrap();
    let parameters = json!({"host":"devhost","container_id":"old-id","pull":true});
    let context = context();
    let plan = runtime
        .plan(&operation, &parameters, &context)
        .await
        .unwrap();
    let encoded = serde_json::to_value(&plan.changes()[0]).unwrap();
    assert_eq!(encoded["before_digest"], "a".repeat(64));
    assert_eq!(encoded["action"], "recreate_pull");

    let result = runtime
        .execute(
            &operation,
            &parameters,
            &context,
            &plan,
            &authorization(&operation, &plan),
            &CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(result.status(), OperationStatus::Succeeded);
    assert_eq!(*container.mutations.lock().unwrap(), 1);
    assert!(
        result
            .evidence()
            .iter()
            .any(|evidence| format!("{evidence:?}").contains("diff"))
    );
    assert!(
        result
            .evidence()
            .iter()
            .any(|evidence| format!("{evidence:?}").contains("runtime_state"))
    );
}

#[tokio::test]
async fn container_pull_choice_drift_rejects_before_send() {
    let expected = fingerprint("a");
    let container = Arc::new(FakeContainerRecreate {
        fingerprints: Mutex::new(VecDeque::from([expected.clone(), expected])),
        inspections: Mutex::new(VecDeque::new()),
        mutations: Mutex::new(0),
    });
    let compose = Arc::new(FakeComposeRecreate {
        projects: Mutex::new(VecDeque::new()),
        configs: Mutex::new(VecDeque::new()),
        statuses: Mutex::new(VecDeque::new()),
        mutations: Mutex::new(0),
    });
    let runtime = runtime(Arc::clone(&container), compose);
    let operation = OperationName::new("container.recreate").unwrap();
    let planned = json!({"host":"devhost","container_id":"old-id","pull":true});
    let execution = json!({"host":"devhost","container_id":"old-id","pull":false});
    let context = context();
    let plan = runtime.plan(&operation, &planned, &context).await.unwrap();
    let error = runtime
        .execute(
            &operation,
            &execution,
            &context,
            &plan,
            &authorization(&operation, &plan),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, ExecutionError::PlanMismatch(_)));
    assert_eq!(*container.mutations.lock().unwrap(), 0);
}

#[tokio::test]
async fn compose_recreate_binds_config_executes_and_verifies() {
    let before = compose_status("running");
    let config = compose_config("api:v1");
    let compose = Arc::new(FakeComposeRecreate {
        projects: Mutex::new(VecDeque::from([projects(), projects()])),
        configs: Mutex::new(VecDeque::from([config.clone(), config.clone(), config])),
        statuses: Mutex::new(VecDeque::from([
            before.clone(),
            before.clone(),
            before,
            compose_status("running"),
        ])),
        mutations: Mutex::new(0),
    });
    let container = Arc::new(FakeContainerRecreate {
        fingerprints: Mutex::new(VecDeque::new()),
        inspections: Mutex::new(VecDeque::new()),
        mutations: Mutex::new(0),
    });
    let runtime = runtime(container, Arc::clone(&compose));
    let operation = OperationName::new("compose.recreate").unwrap();
    let parameters = json!({"host":"devhost","project":"soma"});
    let context = context();
    let plan = runtime
        .plan(&operation, &parameters, &context)
        .await
        .unwrap();
    assert_eq!(plan.changes()[0].action(), "force_recreate");

    let result = runtime
        .execute(
            &operation,
            &parameters,
            &context,
            &plan,
            &authorization(&operation, &plan),
            &CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(result.status(), OperationStatus::Succeeded);
    assert_eq!(*compose.mutations.lock().unwrap(), 1);
    assert!(result.evidence().len() >= 2);
}

#[tokio::test]
async fn absent_recreate_ports_fail_closed_before_inspection() {
    let runtime: SynapseMutationRuntime = crate::mutation_pull_test_support::runtime(None, None);
    let operation = OperationName::new("container.recreate").unwrap();
    let error = runtime
        .plan(
            &operation,
            &json!({"host":"devhost","container_id":"old-id"}),
            &context(),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        ExecutionError::MutationPortUnavailable {
            domain: "recreate",
            ..
        }
    ));
}
