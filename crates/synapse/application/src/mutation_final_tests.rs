use std::path::PathBuf;

use soma_fleet::HostRecord;

use super::*;
use crate::mutation_final_contract::{final_operation, transfer_target};

#[test]
fn final_operation_set_is_closed() {
    for name in [
        "docker.rmi",
        "docker.prune",
        "compose.down",
        "files.transfer",
    ] {
        assert!(final_operation(&OperationName::new(name).unwrap()));
    }
    assert!(!final_operation(
        &OperationName::new("docker.pull").unwrap()
    ));
}

#[test]
fn transfer_target_binds_both_host_revisions_and_paths() {
    let source = HostRecord::new(
        soma_fleet::HostId::new("source").unwrap(),
        soma_fleet::HostEndpoint::Local,
    );
    let destination = HostRecord::new(
        soma_fleet::HostId::new("destination").unwrap(),
        soma_fleet::HostEndpoint::Local,
    );
    let target = transfer_target(
        &source,
        &PathBuf::from("/src/file"),
        &destination,
        &PathBuf::from("/dst/file"),
    )
    .unwrap();
    assert_eq!(target.host(), Some("destination"));
    assert_eq!(target.parent().unwrap().host(), Some("source"));
}

use std::sync::Arc;

use serde_json::json;
use soma_ops::OperationStatus;
use tokio_util::sync::CancellationToken;

use crate::mutation_final_test_docker::{FakeCleanup, cleanup_image};
use crate::mutation_final_test_io::{FakeComposeDown, FakeTransfer, op, runtime};
use crate::mutation_pull_test_support::{authorization, context, host};

fn final_runtime(
    cleanup: Arc<FakeCleanup>,
    compose: Arc<FakeComposeDown>,
    transfer: Arc<FakeTransfer>,
) -> SynapseMutationRuntime {
    runtime(cleanup, compose, transfer, true)
}

#[tokio::test]
async fn docker_rmi_plans_removes_and_verifies_absence() {
    let cleanup = Arc::new(FakeCleanup::new(cleanup_image(&host())));
    let compose = Arc::new(FakeComposeDown::new());
    let transfer = Arc::new(FakeTransfer::new());
    let runtime = final_runtime(cleanup.clone(), compose, transfer);
    let operation = op("docker.rmi");
    let parameters = json!({"host":"devhost","image":"app:v1","force":true});
    let context = context();
    let plan = runtime
        .plan(&operation, &parameters, &context)
        .await
        .unwrap();
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
    assert_eq!(result.output().unwrap()["changed"], true);
    assert_eq!(cleanup.remove_calls(), 1);
}

#[tokio::test]
async fn docker_prune_binds_inventory_and_verifies_deleted_images() {
    let cleanup = Arc::new(FakeCleanup::new(cleanup_image(&host())));
    let compose = Arc::new(FakeComposeDown::new());
    let transfer = Arc::new(FakeTransfer::new());
    let runtime = final_runtime(cleanup.clone(), compose, transfer);
    let operation = op("docker.prune");
    let parameters = json!({"host":"devhost","prune_target":"images","force":true});
    let context = context();
    let plan = runtime
        .plan(&operation, &parameters, &context)
        .await
        .unwrap();
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
    assert_eq!(
        result.output().unwrap()["details"]["after_counts"]["images"],
        0
    );
    assert_eq!(cleanup.prune_calls(), 1);
}

#[tokio::test]
async fn compose_down_binds_service_set_and_verifies_empty_status() {
    let cleanup = Arc::new(FakeCleanup::new(cleanup_image(&host())));
    let compose = Arc::new(FakeComposeDown::new());
    let transfer = Arc::new(FakeTransfer::new());
    let runtime = final_runtime(cleanup, compose.clone(), transfer);
    let operation = op("compose.down");
    let parameters = json!({"host":"devhost","project":"soma"});
    let context = context();
    let plan = runtime
        .plan(&operation, &parameters, &context)
        .await
        .unwrap();
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
    assert_eq!(result.output().unwrap()["details"]["services_after"], 0);
    assert_eq!(compose.calls(), 1);
}

#[tokio::test]
async fn files_transfer_binds_both_hosts_and_returns_verified_artifact() {
    let cleanup = Arc::new(FakeCleanup::new(cleanup_image(&host())));
    let compose = Arc::new(FakeComposeDown::new());
    let transfer = Arc::new(FakeTransfer::new());
    let runtime = final_runtime(cleanup, compose, transfer.clone());
    let operation = op("files.transfer");
    let parameters = json!({
        "source_host":"source",
        "source_path":"/source/payload.bin",
        "dest_host":"destination",
        "dest_path":"/destination/payload.bin"
    });
    let context = context();
    let plan = runtime
        .plan(&operation, &parameters, &context)
        .await
        .unwrap();
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
    assert_eq!(result.output().unwrap()["verified"], true);
    assert_eq!(result.output().unwrap()["bytes"], 7);
    assert_eq!(transfer.calls(), 1);
}

#[tokio::test]
async fn final_parameter_drift_rejects_before_destructive_send() {
    let cleanup = Arc::new(FakeCleanup::new(cleanup_image(&host())));
    let runtime = final_runtime(
        cleanup.clone(),
        Arc::new(FakeComposeDown::new()),
        Arc::new(FakeTransfer::new()),
    );
    let operation = op("docker.rmi");
    let parameters = json!({"host":"devhost","image":"app:v1","force":true});
    let context = context();
    let plan = runtime
        .plan(&operation, &parameters, &context)
        .await
        .unwrap();
    let drifted = json!({"host":"devhost","image":"app:v2","force":true});
    assert!(
        runtime
            .execute(
                &operation,
                &drifted,
                &context,
                &plan,
                &authorization(&operation, &plan),
                &CancellationToken::new(),
            )
            .await
            .is_err()
    );
    assert_eq!(cleanup.remove_calls(), 0);
}

#[tokio::test]
async fn absent_final_ports_fail_closed_before_send() {
    let cleanup = Arc::new(FakeCleanup::new(cleanup_image(&host())));
    let runtime = runtime(
        cleanup.clone(),
        Arc::new(FakeComposeDown::new()),
        Arc::new(FakeTransfer::new()),
        false,
    );
    let operation = op("docker.rmi");
    let error = runtime
        .plan(
            &operation,
            &json!({"host":"devhost","image":"app:v1","force":true}),
            &context(),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        ExecutionError::MutationPortUnavailable { .. }
    ));
    assert_eq!(cleanup.remove_calls(), 0);
}
