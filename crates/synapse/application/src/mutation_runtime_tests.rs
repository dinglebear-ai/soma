use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use soma_fleet::{
    FleetResult, HostEndpoint, HostId, HostRecord, HostRepository, SshEndpoint, TopologySnapshot,
};
use soma_infra::{
    ContainerInspect, ContainerLifecycleAction, ContainerLifecycleMutator,
    ContainerLifecycleRequest, ContainerMutationReceipt, ContainerProcessTable, ContainerReader,
    ContainerState, ContainerSummary, DockerMutationClient, DockerMutationClientProvider,
    InfraError, InfraResult, MutationFailure, MutationResult, MutationVerificationPolicy,
};
use soma_ops::{
    AuthorizationEvidence, AuthorizationScope, IdempotencyKey, MutationSendState, OperationContext,
    OperationName, OperationStatus, ProducerRef, RetryClass, Timestamp,
};

use super::*;

struct MutableHosts {
    snapshot: Mutex<TopologySnapshot>,
}

impl MutableHosts {
    fn new(host: HostRecord) -> Self {
        Self {
            snapshot: Mutex::new(TopologySnapshot::new([host]).unwrap()),
        }
    }

    fn replace(&self, host: HostRecord) {
        *self.snapshot.lock().unwrap() = TopologySnapshot::new([host]).unwrap();
    }
}

#[async_trait]
impl HostRepository for MutableHosts {
    async fn snapshot(&self) -> FleetResult<TopologySnapshot> {
        Ok(self.snapshot.lock().unwrap().clone())
    }
}

struct FakeDocker {
    inspections: Mutex<VecDeque<Result<ContainerState, InfraError>>>,
    actions: Mutex<Vec<ContainerLifecycleAction>>,
    mutation_failure: Option<MutationFailure>,
}

impl FakeDocker {
    fn states(states: impl IntoIterator<Item = ContainerState>) -> Self {
        Self {
            inspections: Mutex::new(states.into_iter().map(Ok).collect()),
            actions: Mutex::new(Vec::new()),
            mutation_failure: None,
        }
    }

    fn failing(failure: MutationFailure) -> Self {
        Self {
            inspections: Mutex::new(VecDeque::from([Ok(ContainerState::Exited)])),
            actions: Mutex::new(Vec::new()),
            mutation_failure: Some(failure),
        }
    }
}

#[async_trait]
impl ContainerReader for FakeDocker {
    async fn list_containers(
        &self,
        _host: &HostRecord,
        _options: &soma_infra::ContainerListOptions,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ContainerSummary>> {
        Ok(Vec::new())
    }

    async fn inspect_container(
        &self,
        host: &HostRecord,
        container: &str,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ContainerInspect> {
        let state = self
            .inspections
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(ContainerState::Unknown("fixture-exhausted".into())))?;
        Ok(ContainerInspect {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            id: Some(container.into()),
            name: Some(container.into()),
            created: None,
            path: None,
            args: Vec::new(),
            image: None,
            state,
            pid: None,
            exit_code: None,
            restart_count: None,
            labels: BTreeMap::new(),
        })
    }

    async fn top_container(
        &self,
        host: &HostRecord,
        container: &str,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ContainerProcessTable> {
        Ok(ContainerProcessTable {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            container: container.into(),
            titles: Vec::new(),
            processes: Vec::new(),
        })
    }
}

#[async_trait]
impl ContainerLifecycleMutator for FakeDocker {
    async fn mutate_container(
        &self,
        host: &HostRecord,
        request: &ContainerLifecycleRequest,
        _cancellation: &CancellationToken,
    ) -> MutationResult<ContainerMutationReceipt> {
        self.actions.lock().unwrap().push(request.action());
        if let Some(failure) = &self.mutation_failure {
            return Err(failure.clone());
        }
        Ok(ContainerMutationReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            container: request.container().into(),
            action: request.action(),
            send_state: MutationSendState::Sent,
        })
    }
}

struct FakeProvider {
    client: Arc<FakeDocker>,
}

#[async_trait]
impl DockerMutationClientProvider for FakeProvider {
    async fn mutation_client(
        &self,
        _host: &HostRecord,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Arc<dyn DockerMutationClient>> {
        Ok(self.client.clone())
    }
}

fn local_host() -> HostRecord {
    HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local)
}

fn ssh_host() -> HostRecord {
    HostRecord::new(
        HostId::new("dookie").unwrap(),
        HostEndpoint::Ssh(SshEndpoint::new("dookie.internal").unwrap()),
    )
}

fn mutation_runtime(hosts: Arc<MutableHosts>, client: Arc<FakeDocker>) -> SynapseMutationRuntime {
    SynapseMutationRuntime::with_lifecycle_engine(
        SynapseMutationPorts {
            hosts,
            docker: Arc::new(FakeProvider { client }),
            compose: None,
            artifacts: None,
            compose_pull: None,
            builds: None,
            recreate: None,
            exec: None,
            final_mutations: None,
        },
        soma_infra::ContainerLifecycleEngine::new(
            MutationVerificationPolicy::new(1, Duration::ZERO).unwrap(),
        ),
    )
}

fn mutation_context(idempotent: bool) -> OperationContext {
    let mut context = OperationContext::new().with_deadline(Timestamp::from_unix_millis(
        Timestamp::now().unix_millis() + 20_000,
    ));
    if idempotent {
        context = context.with_idempotency_key(IdempotencyKey::new("test-request").unwrap());
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
    .with_confirmation_ref("test-confirmation")
    .unwrap()
}

async fn plan_and_execute(
    name: &str,
    before: ContainerState,
    after: ContainerState,
) -> soma_ops::OperationResult {
    let operation = OperationName::new(name).unwrap();
    let idempotent = name != "container.restart";
    let context = mutation_context(idempotent);
    let hosts = Arc::new(MutableHosts::new(local_host()));
    let client = Arc::new(FakeDocker::states([before, after]));
    let runtime = mutation_runtime(hosts, client);
    let parameters = json!({"host":"dookie","container_id":"soma"});
    let plan = runtime
        .plan(&operation, &parameters, &context)
        .await
        .unwrap();
    let authorization = authorization(&operation, &plan);
    runtime
        .execute(
            &operation,
            &parameters,
            &context,
            &plan,
            &authorization,
            &CancellationToken::new(),
        )
        .await
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn all_reversible_lifecycle_operations_plan_execute_and_verify() {
    for (name, before, after) in [
        (
            "container.start",
            ContainerState::Exited,
            ContainerState::Running,
        ),
        (
            "container.stop",
            ContainerState::Running,
            ContainerState::Exited,
        ),
        (
            "container.restart",
            ContainerState::Running,
            ContainerState::Running,
        ),
        (
            "container.pause",
            ContainerState::Running,
            ContainerState::Paused,
        ),
        (
            "container.resume",
            ContainerState::Paused,
            ContainerState::Running,
        ),
    ] {
        let result = plan_and_execute(name, before, after).await;
        assert_eq!(result.status(), OperationStatus::Succeeded, "{name}");
        assert_eq!(
            result.mutation_send_state(),
            MutationSendState::Sent,
            "{name}"
        );
        assert_eq!(result.retry(), RetryClass::Never, "{name}");
        let output = result.output().unwrap();
        assert_eq!(output["action"], name);
        assert_eq!(output["changed"], true);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn already_satisfied_state_returns_verified_noop() {
    let operation = OperationName::new("container.start").unwrap();
    let context = mutation_context(true);
    let hosts = Arc::new(MutableHosts::new(local_host()));
    let client = Arc::new(FakeDocker::states([ContainerState::Running]));
    let runtime = mutation_runtime(hosts, Arc::clone(&client));
    let parameters = json!({"host":"dookie","container_id":"soma"});
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
    assert_eq!(result.mutation_send_state(), MutationSendState::NotSent);
    assert_eq!(result.output().unwrap()["changed"], false);
    assert!(client.actions.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn admission_rejects_missing_idempotency_and_confirmation() {
    let operation = OperationName::new("container.start").unwrap();
    let context = mutation_context(false);
    let hosts = Arc::new(MutableHosts::new(local_host()));
    let runtime = mutation_runtime(hosts, Arc::new(FakeDocker::states([])));
    let parameters = json!({"host":"dookie","container_id":"soma"});
    let plan = runtime
        .plan(&operation, &parameters, &context)
        .await
        .unwrap();
    let confirmed = authorization(&operation, &plan);
    assert!(matches!(
        runtime
            .execute(
                &operation,
                &parameters,
                &context,
                &plan,
                &confirmed,
                &CancellationToken::new(),
            )
            .await,
        Err(ExecutionError::MissingIdempotencyKey)
    ));

    let context = mutation_context(true);
    let plan = runtime
        .plan(&operation, &parameters, &context)
        .await
        .unwrap();
    let now = Timestamp::now().unix_millis();
    let unconfirmed = AuthorizationEvidence::new(
        ProducerRef::new("synapse-policy", "1").unwrap(),
        AuthorizationScope::new(operation.clone(), plan.target().clone()),
        Timestamp::from_unix_millis(now - 1_000),
        Timestamp::from_unix_millis(now + 10_000),
    )
    .unwrap()
    .with_plan_fingerprint(plan.fingerprint().clone());
    assert!(matches!(
        runtime
            .execute(
                &operation,
                &parameters,
                &context,
                &plan,
                &unconfirmed,
                &CancellationToken::new(),
            )
            .await,
        Err(ExecutionError::ConfirmationRequired)
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn topology_change_invalidates_the_authorized_plan() {
    let operation = OperationName::new("container.pause").unwrap();
    let context = mutation_context(true);
    let hosts = Arc::new(MutableHosts::new(local_host()));
    let runtime = mutation_runtime(Arc::clone(&hosts), Arc::new(FakeDocker::states([])));
    let parameters = json!({"host":"dookie","container_id":"soma"});
    let plan = runtime
        .plan(&operation, &parameters, &context)
        .await
        .unwrap();
    let authorization = authorization(&operation, &plan);
    hosts.replace(ssh_host());
    assert!(matches!(
        runtime
            .execute(
                &operation,
                &parameters,
                &context,
                &plan,
                &authorization,
                &CancellationToken::new(),
            )
            .await,
        Err(ExecutionError::PlanMismatch(message)) if message.contains("target") || message.contains("topology")
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn unknown_send_state_and_failed_verification_become_failed_terminal_results() {
    let operation = OperationName::new("container.start").unwrap();
    let parameters = json!({"host":"dookie","container_id":"soma"});
    let context = mutation_context(true);
    let hosts = Arc::new(MutableHosts::new(local_host()));
    let failure = MutationFailure::new(
        MutationSendState::Unknown,
        InfraError::Docker("connection reset".into()),
    );
    let runtime = mutation_runtime(hosts, Arc::new(FakeDocker::failing(failure)));
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
    assert_eq!(result.mutation_send_state(), MutationSendState::Unknown);
    assert_eq!(result.retry(), RetryClass::Safe);

    let runtime = mutation_runtime(
        Arc::new(MutableHosts::new(local_host())),
        Arc::new(FakeDocker::states([
            ContainerState::Running,
            ContainerState::Exited,
        ])),
    );
    let operation = OperationName::new("container.restart").unwrap();
    let context = mutation_context(false);
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
async fn expired_authorization_is_rejected_before_provider_access() {
    let operation = OperationName::new("container.stop").unwrap();
    let context = mutation_context(true);
    let runtime = mutation_runtime(
        Arc::new(MutableHosts::new(local_host())),
        Arc::new(FakeDocker::states([])),
    );
    let parameters = json!({"host":"dookie","container_id":"soma"});
    let plan = runtime
        .plan(&operation, &parameters, &context)
        .await
        .unwrap();
    let now = Timestamp::now().unix_millis();
    let expired = AuthorizationEvidence::new(
        ProducerRef::new("synapse-policy", "1").unwrap(),
        AuthorizationScope::new(operation.clone(), plan.target().clone()),
        Timestamp::from_unix_millis(now - 2_000),
        Timestamp::from_unix_millis(now - 1_000),
    )
    .unwrap()
    .with_plan_fingerprint(plan.fingerprint().clone())
    .with_confirmation_ref("expired-confirmation")
    .unwrap();
    assert!(matches!(
        runtime
            .execute(
                &operation,
                &parameters,
                &context,
                &plan,
                &expired,
                &CancellationToken::new(),
            )
            .await,
        Err(ExecutionError::Authorization(
            soma_ops::AuthorizationError::Expired
        ))
    ));
}
