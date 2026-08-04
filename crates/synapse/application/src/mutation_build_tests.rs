use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use soma_fleet::{HostRecord, TopologySnapshot};
use soma_infra::{
    BuildContextFingerprint, BuildContextInspector, ComposeBuildMutator, ComposeBuildReceipt,
    ComposeBuildRequest, ComposeConfig, ComposeServiceConfig, ImageBuildMutator, ImageBuildReceipt,
    ImageBuildRequest, InfraResult, MutationProgressReporter, MutationResult,
};
use soma_ops::{MutationSendState, OperationName, OperationStatus, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::mutation_pull_test_compose::FakeComposePull;
use crate::mutation_pull_test_support::{
    FakeArtifactProvider, FakeArtifacts, StaticHosts, UnusedLifecycle, authorization, context,
    host, image,
};
use crate::{ExecutionError, SynapseBuildPorts, SynapseMutationPorts, SynapseMutationRuntime};

struct Contexts(Mutex<VecDeque<BuildContextFingerprint>>);
#[async_trait]
impl BuildContextInspector for Contexts {
    async fn fingerprint(
        &self,
        _: &HostRecord,
        path: &Path,
        _: Timestamp,
        _: &CancellationToken,
    ) -> InfraResult<BuildContextFingerprint> {
        let mut value = self.0.lock().unwrap().pop_front().unwrap();
        value.path = path.to_path_buf();
        Ok(value)
    }
}
struct ImageBuilder(Mutex<usize>);
#[async_trait]
impl ImageBuildMutator for ImageBuilder {
    async fn build_image(
        &self,
        host: &HostRecord,
        request: &ImageBuildRequest,
        _: &dyn MutationProgressReporter,
        _: &CancellationToken,
    ) -> MutationResult<ImageBuildReceipt> {
        *self.0.lock().unwrap() += 1;
        Ok(ImageBuildReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            tag: request.tag().into(),
            send_state: MutationSendState::Sent,
            stdout: "built".into(),
            stderr: String::new(),
            output_truncated: false,
            progress_delivery_errors: Vec::new(),
        })
    }
}
struct ComposeBuilder(Mutex<usize>);
#[async_trait]
impl ComposeBuildMutator for ComposeBuilder {
    async fn build_compose(
        &self,
        host: &HostRecord,
        request: &ComposeBuildRequest,
        _: &dyn MutationProgressReporter,
        _: &CancellationToken,
    ) -> MutationResult<ComposeBuildReceipt> {
        *self.0.lock().unwrap() += 1;
        Ok(ComposeBuildReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: request.project().name().into(),
            service: request.service().map(str::to_owned),
            send_state: MutationSendState::Sent,
            stdout: "built".into(),
            stderr: String::new(),
            output_truncated: false,
            progress_delivery_errors: Vec::new(),
        })
    }
}
fn fp(value: &str, path: &str) -> BuildContextFingerprint {
    BuildContextFingerprint {
        host: host().id().clone(),
        topology_revision: host().revision().clone(),
        path: path.into(),
        sha256: value.repeat(64),
        file_count: 2,
        byte_count: 20,
    }
}
fn runtime(
    contexts: Arc<Contexts>,
    image_builder: Arc<ImageBuilder>,
    compose_builder: Arc<ComposeBuilder>,
    artifacts: Arc<FakeArtifacts>,
    compose: Arc<FakeComposePull>,
) -> SynapseMutationRuntime {
    SynapseMutationRuntime::new(SynapseMutationPorts {
        hosts: Arc::new(StaticHosts(TopologySnapshot::new([host()]).unwrap())),
        docker: Arc::new(UnusedLifecycle),
        compose: None,
        artifacts: Some(Arc::new(FakeArtifactProvider(artifacts))),
        compose_pull: Some(compose),
        builds: Some(SynapseBuildPorts {
            contexts,
            image: image_builder,
            compose: compose_builder,
        }),
        recreate: None,
        exec: None,
    })
}

#[tokio::test]
async fn docker_build_plans_executes_and_verifies_context_bound_image() {
    let contexts = Arc::new(Contexts(Mutex::new(VecDeque::from([
        fp("a", "/srv/app"),
        fp("a", "/srv/app"),
        fp("a", "/srv/app"),
    ]))));
    let builder = Arc::new(ImageBuilder(Mutex::new(0)));
    let compose_builder = Arc::new(ComposeBuilder(Mutex::new(0)));
    let artifacts = Arc::new(FakeArtifacts {
        containers: Mutex::new(VecDeque::new()),
        images: Mutex::new(VecDeque::from([
            Ok(Vec::new()),
            Ok(vec![image("sha256:build", "app:v1")]),
        ])),
        pull_count: Mutex::new(0),
        progress_delivery_error: false,
    });
    let compose = Arc::new(FakeComposePull {
        config: ComposeConfig {
            host: host().id().clone(),
            topology_revision: host().revision().clone(),
            project: "soma".into(),
            services: BTreeMap::new(),
            networks: Vec::new(),
            volumes: Vec::new(),
        },
        pull_count: Mutex::new(0),
    });
    let runtime = runtime(
        contexts,
        builder.clone(),
        compose_builder,
        artifacts,
        compose,
    );
    let operation = OperationName::new("docker.build").unwrap();
    let params = json!({"host":"dookie","context":"/srv/app","tag":"app:v1","dockerfile":"Dockerfile","no_cache":true});
    let ctx = context();
    let plan = runtime.plan(&operation, &params, &ctx).await.unwrap();
    assert_eq!(
        serde_json::to_value(&plan.changes()[0]).unwrap()["before_digest"],
        "a".repeat(64)
    );
    let result = runtime
        .execute(
            &operation,
            &params,
            &ctx,
            &plan,
            &authorization(&operation, &plan),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result.status(), OperationStatus::Succeeded);
    assert_eq!(*builder.0.lock().unwrap(), 1);
    assert_eq!(result.artifacts().len(), 1);
    assert!(
        result
            .evidence()
            .iter()
            .any(|e| format!("{e:?}").contains("source_context"))
    );
}

#[tokio::test]
async fn compose_build_binds_service_context_and_verifies_image() {
    let contexts = Arc::new(Contexts(Mutex::new(VecDeque::from([
        fp("a", "/srv/soma/api"),
        fp("a", "/srv/soma/api"),
        fp("a", "/srv/soma/api"),
    ]))));
    let builder = Arc::new(ComposeBuilder(Mutex::new(0)));
    let artifacts = Arc::new(FakeArtifacts {
        containers: Mutex::new(VecDeque::new()),
        images: Mutex::new(VecDeque::from([
            Ok(Vec::new()),
            Ok(vec![image("sha256:compose", "soma-api:v1")]),
        ])),
        pull_count: Mutex::new(0),
        progress_delivery_error: false,
    });
    let compose = Arc::new(FakeComposePull {
        config: ComposeConfig {
            host: host().id().clone(),
            topology_revision: host().revision().clone(),
            project: "soma".into(),
            services: BTreeMap::from([(
                "api".into(),
                ComposeServiceConfig {
                    image: Some("soma-api:v1".into()),
                    build_context: Some("api".into()),
                    profiles: Vec::new(),
                },
            )]),
            networks: Vec::new(),
            volumes: Vec::new(),
        },
        pull_count: Mutex::new(0),
    });
    let runtime = runtime(
        contexts,
        Arc::new(ImageBuilder(Mutex::new(0))),
        builder.clone(),
        artifacts,
        compose,
    );
    let operation = OperationName::new("compose.build").unwrap();
    let params = json!({"host":"dookie","project":"soma","service":"api"});
    let ctx = context();
    let plan = runtime.plan(&operation, &params, &ctx).await.unwrap();
    let result = runtime
        .execute(
            &operation,
            &params,
            &ctx,
            &plan,
            &authorization(&operation, &plan),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(result.status(), OperationStatus::Succeeded);
    assert_eq!(*builder.0.lock().unwrap(), 1);
    assert_eq!(result.artifacts().len(), 1);
}

#[tokio::test]
async fn absent_build_ports_fail_closed_before_context_access() {
    let runtime = crate::mutation_pull_test_support::runtime(None, None);
    let operation = OperationName::new("docker.build").unwrap();
    let params = json!({"host":"dookie","context":"/srv/app","tag":"app:v1"});
    let error = runtime
        .plan(&operation, &params, &context())
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        ExecutionError::MutationPortUnavailable {
            domain: "build",
            ..
        }
    ));
}
