use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use soma_fleet::{HostEndpoint, HostId};

use super::*;
use crate::{
    ContainerInspect, ContainerLifecycleAction, ContainerLifecycleMutator, ContainerListOptions,
    ContainerMutationReceipt, ContainerProcessTable, ContainerReader, ContainerSummary, InfraError,
};

struct FakeClient {
    inspections: Mutex<VecDeque<Result<ContainerState, InfraError>>>,
    mutations: Mutex<Vec<ContainerLifecycleAction>>,
    mutation_failure: Option<MutationFailure>,
}

impl FakeClient {
    fn with_states(states: impl IntoIterator<Item = ContainerState>) -> Self {
        Self {
            inspections: Mutex::new(states.into_iter().map(Ok).collect()),
            mutations: Mutex::new(Vec::new()),
            mutation_failure: None,
        }
    }

    fn failing(failure: MutationFailure) -> Self {
        Self {
            inspections: Mutex::new(VecDeque::from([Ok(ContainerState::Exited)])),
            mutations: Mutex::new(Vec::new()),
            mutation_failure: Some(failure),
        }
    }
}

#[async_trait]
impl ContainerReader for FakeClient {
    async fn list_containers(
        &self,
        _host: &HostRecord,
        _options: &ContainerListOptions,
        _cancellation: &CancellationToken,
    ) -> crate::InfraResult<Vec<ContainerSummary>> {
        Ok(Vec::new())
    }

    async fn inspect_container(
        &self,
        host: &HostRecord,
        container: &str,
        _cancellation: &CancellationToken,
    ) -> crate::InfraResult<ContainerInspect> {
        let state = self
            .inspections
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(ContainerState::Unknown("missing-fixture".into())))?;
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
    ) -> crate::InfraResult<ContainerProcessTable> {
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
impl ContainerLifecycleMutator for FakeClient {
    async fn mutate_container(
        &self,
        host: &HostRecord,
        request: &ContainerLifecycleRequest,
        _cancellation: &CancellationToken,
    ) -> MutationResult<ContainerMutationReceipt> {
        self.mutations.lock().unwrap().push(request.action());
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

fn host() -> HostRecord {
    HostRecord::new(HostId::new("devhost").unwrap(), HostEndpoint::Local)
}

fn request(action: ContainerLifecycleAction) -> ContainerLifecycleRequest {
    ContainerLifecycleRequest::new(
        "soma",
        action,
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
    )
    .unwrap()
}

fn engine(attempts: u8) -> ContainerLifecycleEngine {
    ContainerLifecycleEngine::new(
        MutationVerificationPolicy::new(attempts, Duration::ZERO).unwrap(),
    )
}

#[tokio::test(flavor = "current_thread")]
async fn already_satisfied_state_is_verified_without_send() {
    let client = FakeClient::with_states([ContainerState::Running]);
    let outcome = engine(1)
        .execute(
            &client,
            &host(),
            &request(ContainerLifecycleAction::Start),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(!outcome.changed);
    assert_eq!(outcome.send_state, MutationSendState::NotSent);
    assert_eq!(outcome.verification_status, VerificationStatus::Verified);
    assert!(client.mutations.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn mutation_is_sent_then_verified_from_independent_read() {
    let client = FakeClient::with_states([ContainerState::Running, ContainerState::Paused]);
    let outcome = engine(1)
        .execute(
            &client,
            &host(),
            &request(ContainerLifecycleAction::Pause),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(outcome.changed);
    assert_eq!(outcome.send_state, MutationSendState::Sent);
    assert_eq!(outcome.after, Some(ContainerState::Paused));
    assert_eq!(outcome.verification_status, VerificationStatus::Verified);
    assert_eq!(
        *client.mutations.lock().unwrap(),
        vec![ContainerLifecycleAction::Pause]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn sent_mutation_with_wrong_post_state_is_not_reported_as_success() {
    let client = FakeClient::with_states([ContainerState::Running, ContainerState::Exited]);
    let outcome = engine(1)
        .execute(
            &client,
            &host(),
            &request(ContainerLifecycleAction::Restart),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.send_state, MutationSendState::Sent);
    assert_eq!(outcome.verification_status, VerificationStatus::Failed);
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_before_admission_is_not_sent() {
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let failure = engine(1)
        .execute(
            &FakeClient::with_states([]),
            &host(),
            &request(ContainerLifecycleAction::Stop),
            &cancellation,
        )
        .await
        .unwrap_err();
    assert_eq!(failure.send_state(), MutationSendState::NotSent);
}

#[tokio::test(flavor = "current_thread")]
async fn driver_send_uncertainty_is_preserved() {
    let failure = MutationFailure::new(
        MutationSendState::Unknown,
        InfraError::Docker("connection reset".into()),
    );
    let client = FakeClient::failing(failure);
    let failure = engine(1)
        .execute(
            &client,
            &host(),
            &request(ContainerLifecycleAction::Start),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(failure.send_state(), MutationSendState::Unknown);
}

#[test]
fn mutation_requests_and_verification_policies_are_bounded() {
    let deadline = Timestamp::now();
    assert!(ContainerLifecycleRequest::new("", ContainerLifecycleAction::Start, deadline).is_err());
    assert!(MutationVerificationPolicy::new(0, Duration::ZERO).is_err());
    assert!(MutationVerificationPolicy::new(21, Duration::ZERO).is_err());
}
