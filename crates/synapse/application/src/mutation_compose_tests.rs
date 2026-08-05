use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use soma_fleet::{FleetResult, HostEndpoint, HostId, HostRecord, HostRepository, TopologySnapshot};
use soma_infra::{
    ComposeConfig, ComposeInspector, ComposeLogRequest, ComposeLogs, ComposeMutationAction,
    ComposeMutationClient, ComposeMutationEngine, ComposeMutationReceipt, ComposeMutationRequest,
    ComposeMutator, ComposeProject, ComposeProjectRef, ComposeServiceStatus, ComposeStatus,
    ContainerLifecycleEngine, DockerMutationClient, DockerMutationClientProvider, InfraError,
    InfraResult, MutationFailure, MutationResult, MutationVerificationPolicy,
};
use soma_ops::{
    AuthorizationEvidence, AuthorizationScope, IdempotencyKey, MutationSendState, OperationContext,
    OperationName, OperationStatus, ProducerRef, RetryClass, Timestamp,
};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::{SynapseMutationPorts, SynapseMutationRuntime};

struct StaticHosts(TopologySnapshot);

#[async_trait]
impl HostRepository for StaticHosts {
    async fn snapshot(&self) -> FleetResult<TopologySnapshot> {
        Ok(self.0.clone())
    }
}

struct UnusedDocker;

#[async_trait]
impl DockerMutationClientProvider for UnusedDocker {
    async fn mutation_client(
        &self,
        host: &HostRecord,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Arc<dyn DockerMutationClient>> {
        Err(InfraError::UnsupportedTarget {
            domain: "docker-mutation",
            host: host.id().clone(),
        })
    }
}

struct FakeCompose {
    statuses: Mutex<VecDeque<InfraResult<ComposeStatus>>>,
    actions: Mutex<Vec<ComposeMutationAction>>,
    project_reads: Mutex<usize>,
    failure: Option<MutationFailure>,
}

impl FakeCompose {
    fn statuses(statuses: impl IntoIterator<Item = ComposeStatus>) -> Self {
        Self {
            statuses: Mutex::new(statuses.into_iter().map(Ok).collect()),
            actions: Mutex::new(Vec::new()),
            project_reads: Mutex::new(0),
            failure: None,
        }
    }
}

#[async_trait]
impl ComposeInspector for FakeCompose {
    async fn list_projects(
        &self,
        host: &HostRecord,
        _deadline: Timestamp,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ComposeProject>> {
        *self.project_reads.lock().unwrap() += 1;
        Ok(vec![ComposeProject {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            name: "soma".into(),
            status: Some("running".into()),
            config_files: vec!["/srv/soma/compose.yaml".into()],
        }])
    }

    async fn status(
        &self,
        _host: &HostRecord,
        _project: &ComposeProjectRef,
        _service: Option<&str>,
        _deadline: Timestamp,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ComposeStatus> {
        self.statuses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| {
                Err(InfraError::Parse {
                    domain: "compose",
                    message: "status fixture exhausted".into(),
                })
            })
    }

    async fn config(
        &self,
        _host: &HostRecord,
        _project: &ComposeProjectRef,
        _deadline: Timestamp,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ComposeConfig> {
        Err(InfraError::Parse {
            domain: "compose",
            message: "config not used by mutation tests".into(),
        })
    }

    async fn logs(
        &self,
        _host: &HostRecord,
        _project: &ComposeProjectRef,
        _request: &ComposeLogRequest,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ComposeLogs> {
        Err(InfraError::Parse {
            domain: "compose",
            message: "logs not used by mutation tests".into(),
        })
    }
}

#[async_trait]
impl ComposeMutator for FakeCompose {
    async fn mutate_compose(
        &self,
        host: &HostRecord,
        request: &ComposeMutationRequest,
        _cancellation: &CancellationToken,
    ) -> MutationResult<ComposeMutationReceipt> {
        self.actions.lock().unwrap().push(request.action());
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        Ok(ComposeMutationReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: request.project().name().into(),
            action: request.action(),
            send_state: MutationSendState::Sent,
        })
    }
}

fn host() -> HostRecord {
    HostRecord::new(HostId::new("devhost").unwrap(), HostEndpoint::Local)
}

fn status(state: &str, health: Option<&str>) -> ComposeStatus {
    let host = host();
    ComposeStatus {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        project: "soma".into(),
        services: vec![ComposeServiceStatus {
            service: "api".into(),
            container_name: Some("soma-api".into()),
            state: Some(state.into()),
            health: health.map(str::to_owned),
            exit_code: Some(0),
            image: Some("soma:latest".into()),
        }],
    }
}

fn runtime(compose: Option<Arc<dyn ComposeMutationClient>>) -> SynapseMutationRuntime {
    SynapseMutationRuntime::with_engines(
        SynapseMutationPorts {
            hosts: Arc::new(StaticHosts(TopologySnapshot::new([host()]).unwrap())),
            docker: Arc::new(UnusedDocker),
            compose,
            artifacts: None,
            compose_pull: None,
            builds: None,
            recreate: None,
            exec: None,
        },
        ContainerLifecycleEngine::default(),
        ComposeMutationEngine::new(MutationVerificationPolicy::new(1, Duration::ZERO).unwrap()),
    )
}

fn context(idempotent: bool) -> OperationContext {
    let mut context = OperationContext::new().with_deadline(Timestamp::from_unix_millis(
        Timestamp::now().unix_millis() + 20_000,
    ));
    if idempotent {
        context = context.with_idempotency_key(IdempotencyKey::new("compose-request").unwrap());
    }
    context
}

fn authorization(
    operation: &OperationName,
    plan: &soma_ops::OperationPlan,
) -> AuthorizationEvidence {
    let now = Timestamp::now().unix_millis();
    AuthorizationEvidence::new(
        ProducerRef::new("synapse-policy", "1").unwrap(),
        AuthorizationScope::new(operation.clone(), plan.target().clone()),
        Timestamp::from_unix_millis(now - 1_000),
        Timestamp::from_unix_millis(now + 10_000),
    )
    .unwrap()
    .with_plan_fingerprint(plan.fingerprint().clone())
    .with_confirmation_ref("compose-confirmation")
    .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn compose_up_and_restart_plan_execute_and_verify() {
    for (name, idempotent) in [("compose.up", true), ("compose.restart", false)] {
        let compose = Arc::new(FakeCompose::statuses([
            status("exited", None),
            status("running", Some("healthy")),
        ]));
        let runtime = runtime(Some(compose.clone()));
        let operation = OperationName::new(name).unwrap();
        let context = context(idempotent);
        let parameters = json!({"host":"devhost","project":"soma"});
        let plan = runtime
            .plan(&operation, &parameters, &context)
            .await
            .unwrap();
        assert_eq!(
            plan.verification().unwrap().operation().as_str(),
            "compose.status"
        );
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
        assert_eq!(result.status(), OperationStatus::Succeeded, "{name}");
        assert_eq!(
            result.mutation_send_state(),
            MutationSendState::Sent,
            "{name}"
        );
        assert_eq!(result.retry(), RetryClass::Never, "{name}");
        assert_eq!(result.output().unwrap()["action"], name);
        assert_eq!(compose.actions.lock().unwrap().len(), 1);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn compose_verification_failure_is_a_failed_terminal_result() {
    let compose = Arc::new(FakeCompose::statuses([
        status("running", Some("healthy")),
        status("running", Some("unhealthy")),
    ]));
    let runtime = runtime(Some(compose));
    let operation = OperationName::new("compose.restart").unwrap();
    let context = context(false);
    let parameters = json!({"host":"devhost","project":"soma"});
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
    assert_eq!(result.status(), OperationStatus::Failed);
    assert_eq!(result.mutation_send_state(), MutationSendState::Sent);
    assert_eq!(result.retry(), RetryClass::Never);
}

#[tokio::test(flavor = "current_thread")]
async fn absent_compose_port_fails_closed() {
    let runtime = runtime(None);
    let operation = OperationName::new("compose.up").unwrap();
    let parameters = json!({"host":"devhost","project":"soma"});
    let error = runtime
        .plan(&operation, &parameters, &context(true))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        ExecutionError::MutationPortUnavailable {
            domain: "compose",
            ..
        }
    ));
}
