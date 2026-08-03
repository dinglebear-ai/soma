use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde_json::json;
use soma_ops::{MutationSendState, OperationName, OperationStatus, VerificationStatus};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::mutation_pull_test_support::{
    CollectProgress, FailingProgress, FakeArtifacts, FakeComposePull, authorization,
    compose_config, container, context, image, runtime,
};

fn artifact_fixture(
    containers: Vec<Vec<soma_infra::ContainerSummary>>,
    images: Vec<Vec<soma_infra::ImageSummary>>,
    progress_delivery_error: bool,
) -> Arc<FakeArtifacts> {
    Arc::new(FakeArtifacts {
        containers: Mutex::new(VecDeque::from(
            containers.into_iter().map(Ok).collect::<Vec<_>>(),
        )),
        images: Mutex::new(VecDeque::from(
            images.into_iter().map(Ok).collect::<Vec<_>>(),
        )),
        pull_count: Mutex::new(0),
        progress_delivery_error,
    })
}

#[tokio::test(flavor = "current_thread")]
async fn docker_pull_plans_reports_progress_and_attaches_verified_artifact() {
    let old = format!("sha256:{}", "a".repeat(64));
    let new = format!("sha256:{}", "b".repeat(64));
    let artifacts = artifact_fixture(
        Vec::new(),
        vec![
            vec![image(&old, "alpine:latest")],
            vec![image(&new, "alpine:latest")],
        ],
        false,
    );
    let runtime = runtime(Some(Arc::clone(&artifacts)), None);
    let operation = OperationName::new("docker.pull").unwrap();
    let parameters = json!({"host":"dookie","image":"alpine:latest"});
    let context = context();
    let plan = runtime
        .plan(&operation, &parameters, &context)
        .await
        .unwrap();
    assert_eq!(plan.changes().len(), 1);
    assert_eq!(
        plan.verification().unwrap().operation().as_str(),
        "docker.images"
    );
    let progress = CollectProgress(Mutex::new(Vec::new()));
    let result = runtime
        .execute_with_progress(
            &operation,
            &parameters,
            &context,
            &plan,
            &authorization(&operation, &plan),
            &progress,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result.status(), OperationStatus::Succeeded);
    assert_eq!(result.mutation_send_state(), MutationSendState::Sent);
    assert_eq!(
        result.verification().unwrap().status(),
        VerificationStatus::Verified
    );
    assert_eq!(result.artifacts().len(), 1);
    assert_eq!(result.evidence().len(), 1);
    assert_eq!(progress.0.lock().unwrap().len(), 1);
    assert_eq!(*artifacts.pull_count.lock().unwrap(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn container_pull_rejects_image_reference_drift_before_send() {
    let artifacts = artifact_fixture(
        vec![
            vec![container("api", "example/api:v1")],
            vec![container("api", "example/api:v2")],
        ],
        Vec::new(),
        false,
    );
    let runtime = runtime(Some(Arc::clone(&artifacts)), None);
    let operation = OperationName::new("container.pull").unwrap();
    let parameters = json!({"host":"dookie","container_id":"api"});
    let context = context();
    let plan = runtime
        .plan(&operation, &parameters, &context)
        .await
        .unwrap();
    assert_eq!(plan.changes()[0].resource().id(), "example/api:v1");
    let error = runtime
        .execute(
            &operation,
            &parameters,
            &context,
            &plan,
            &authorization(&operation, &plan),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, ExecutionError::PlanMismatch(_)));
    assert_eq!(*artifacts.pull_count.lock().unwrap(), 0);
}

#[tokio::test(flavor = "current_thread")]
async fn compose_pull_binds_and_verifies_each_service_image() {
    let old_api = format!("sha256:{}", "c".repeat(64));
    let new_api = format!("sha256:{}", "d".repeat(64));
    let web = format!("sha256:{}", "e".repeat(64));
    let artifacts = artifact_fixture(
        Vec::new(),
        vec![
            vec![
                image(&old_api, "example/api:latest"),
                image(&web, "example/web:v1"),
            ],
            vec![
                image(&new_api, "example/api:latest"),
                image(&web, "example/web:v1"),
            ],
        ],
        false,
    );
    let compose = Arc::new(FakeComposePull {
        config: compose_config(),
        pull_count: Mutex::new(0),
    });
    let runtime = runtime(Some(artifacts), Some(Arc::clone(&compose)));
    let operation = OperationName::new("compose.pull").unwrap();
    let parameters = json!({"host":"dookie","project":"soma"});
    let context = context();
    let plan = runtime
        .plan(&operation, &parameters, &context)
        .await
        .unwrap();
    assert_eq!(plan.changes().len(), 2);
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
    assert_eq!(result.artifacts().len(), 2);
    assert_eq!(*compose.pull_count.lock().unwrap(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn progress_delivery_failure_does_not_rewrite_pull_truth() {
    let digest = format!("sha256:{}", "f".repeat(64));
    let artifacts = artifact_fixture(
        Vec::new(),
        vec![Vec::new(), vec![image(&digest, "alpine:latest")]],
        true,
    );
    let runtime = runtime(Some(artifacts), None);
    let operation = OperationName::new("docker.pull").unwrap();
    let parameters = json!({"host":"dookie","image":"alpine:latest"});
    let context = context();
    let plan = runtime
        .plan(&operation, &parameters, &context)
        .await
        .unwrap();
    let result = runtime
        .execute_with_progress(
            &operation,
            &parameters,
            &context,
            &plan,
            &authorization(&operation, &plan),
            &FailingProgress,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result.status(), OperationStatus::Succeeded);
    assert_eq!(
        result.output().unwrap()["details"]["progress_delivery_errors"][0],
        "progress sink offline"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn absent_artifact_port_fails_closed_before_pull() {
    let runtime = runtime(None, None);
    let operation = OperationName::new("docker.pull").unwrap();
    let parameters = json!({"host":"dookie","image":"alpine:latest"});
    let context = context();
    let plan = runtime
        .plan(&operation, &parameters, &context)
        .await
        .unwrap();
    let error = runtime
        .execute(
            &operation,
            &parameters,
            &context,
            &plan,
            &authorization(&operation, &plan),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        ExecutionError::MutationPortUnavailable {
            domain: "docker-artifact",
            ..
        }
    ));
}
