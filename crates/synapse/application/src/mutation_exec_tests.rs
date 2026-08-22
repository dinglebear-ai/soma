use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde_json::json;
use soma_ops::{MutationSendState, OperationStatus};
use tokio_util::sync::CancellationToken;

use crate::ExecutionError;
use crate::mutation_exec_test_support::{
    FakeContainerExec, FakeHostExec, container_receipt, op, runtime,
};
use crate::mutation_pull_test_support::{authorization, context};

#[tokio::test]
async fn container_exec_binds_argv_and_returns_canonical_output() {
    let container = Arc::new(FakeContainerExec {
        receipts: Mutex::new(VecDeque::from([Ok(container_receipt(0))])),
        calls: Mutex::new(0),
    });
    let hosts = Arc::new(FakeHostExec {
        calls: Mutex::new(Vec::new()),
    });
    let runtime = runtime(container.clone(), hosts, true);
    let operation = op("container.exec");
    let parameters = json!({
        "host":"devhost",
        "container_id":"api",
        "command":["printf","ok"],
        "exec_workdir":"/app",
        "exec_timeout_ms":5000
    });
    let ctx = context();
    let plan = runtime.plan(&operation, &parameters, &ctx).await.unwrap();
    let encoded = serde_json::to_value(&plan).unwrap();
    assert_eq!(
        encoded["changes"][0]["before_digest"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert!(encoded.get("verification").is_none());
    let result = runtime
        .execute(
            &operation,
            &parameters,
            &ctx,
            &plan,
            &authorization(&operation, &plan),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result.status(), OperationStatus::Succeeded);
    assert_eq!(result.output().unwrap()["exit_code"], 0);
    assert_eq!(result.output().unwrap()["stdout"], "ok");
    assert_eq!(*container.calls.lock().unwrap(), 1);
}

#[tokio::test]
async fn host_exec_nonzero_exit_is_a_failed_terminal_result() {
    let container = Arc::new(FakeContainerExec {
        receipts: Mutex::new(VecDeque::new()),
        calls: Mutex::new(0),
    });
    let hosts = Arc::new(FakeHostExec {
        calls: Mutex::new(Vec::new()),
    });
    let runtime = runtime(container, hosts.clone(), true);
    let operation = op("host.exec");
    let parameters = json!({
        "host":"bad",
        "command":"ls",
        "args":["-l","/srv"],
        "path":"/srv",
        "timeout_secs":5
    });
    let ctx = context();
    let plan = runtime.plan(&operation, &parameters, &ctx).await.unwrap();
    let result = runtime
        .execute(
            &operation,
            &parameters,
            &ctx,
            &plan,
            &authorization(&operation, &plan),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result.status(), OperationStatus::Failed);
    assert_eq!(result.mutation_send_state(), MutationSendState::Sent);
    assert_eq!(result.output().unwrap()["exit_code"], 2);
    assert_eq!(hosts.calls.lock().unwrap().as_slice(), &["bad"]);
}

#[tokio::test]
async fn host_exec_many_preserves_sorted_partial_results() {
    let container = Arc::new(FakeContainerExec {
        receipts: Mutex::new(VecDeque::new()),
        calls: Mutex::new(0),
    });
    let hosts = Arc::new(FakeHostExec {
        calls: Mutex::new(Vec::new()),
    });
    let runtime = runtime(container, hosts.clone(), true);
    let operation = op("host.exec_many");
    let parameters = json!({
        "command":"ls",
        "args":["-l"],
        "targets":[
            {"host":"lost","path":"/srv/lost"},
            {"host":"alpha","path":"/srv/alpha"},
            {"host":"bad","path":"/srv/bad"}
        ],
        "timeout_secs":5
    });
    let ctx = context();
    let plan = runtime.plan(&operation, &parameters, &ctx).await.unwrap();
    assert_eq!(plan.steps().len(), 3);
    let result = runtime
        .execute(
            &operation,
            &parameters,
            &ctx,
            &plan,
            &authorization(&operation, &plan),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result.status(), OperationStatus::Failed);
    assert_eq!(result.mutation_send_state(), MutationSendState::Unknown);
    let output = result.output().unwrap();
    assert_eq!(output["results"][0]["target"], "alpha:/srv/alpha");
    assert_eq!(output["results"][1]["target"], "bad:/srv/bad");
    assert_eq!(output["results"][2]["target"], "lost:/srv/lost");
    assert_eq!(output["success_count"], 1);
    assert_eq!(output["failure_count"], 2);
    assert_eq!(hosts.calls.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn execution_parameter_drift_rejects_before_driver_send() {
    let container = Arc::new(FakeContainerExec {
        receipts: Mutex::new(VecDeque::from([Ok(container_receipt(0))])),
        calls: Mutex::new(0),
    });
    let hosts = Arc::new(FakeHostExec {
        calls: Mutex::new(Vec::new()),
    });
    let runtime = runtime(container.clone(), hosts, true);
    let operation = op("container.exec");
    let planned = json!({
        "host":"devhost",
        "container_id":"api",
        "command":["printf","ok"]
    });
    let changed = json!({
        "host":"devhost",
        "container_id":"api",
        "command":["printf","changed"]
    });
    let ctx = context();
    let plan = runtime.plan(&operation, &planned, &ctx).await.unwrap();
    let error = runtime
        .execute(
            &operation,
            &changed,
            &ctx,
            &plan,
            &authorization(&operation, &plan),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, ExecutionError::PlanMismatch(_)));
    assert_eq!(*container.calls.lock().unwrap(), 0);
}

#[tokio::test]
async fn absent_execution_ports_fail_closed_before_send() {
    let container = Arc::new(FakeContainerExec {
        receipts: Mutex::new(VecDeque::new()),
        calls: Mutex::new(0),
    });
    let hosts = Arc::new(FakeHostExec {
        calls: Mutex::new(Vec::new()),
    });
    let runtime = runtime(container, hosts, false);
    let operation = op("host.exec");
    let parameters = json!({"host":"alpha","command":"hostname"});
    let ctx = context();
    let plan = runtime.plan(&operation, &parameters, &ctx).await.unwrap();
    let error = runtime
        .execute(
            &operation,
            &parameters,
            &ctx,
            &plan,
            &authorization(&operation, &plan),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        ExecutionError::MutationPortUnavailable { domain: "exec", .. }
    ));
}

#[test]
fn execution_digest_is_sha2_011_compatible() {
    let digest = super::digest(&serde_json::json!({"command": ["true"]})).unwrap();
    assert_eq!(digest.len(), 64);
    assert!(
        digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    );
}
