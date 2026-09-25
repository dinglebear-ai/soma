use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use soma_fleet::{HostEndpoint, HostId};

use super::*;
use crate::{
    ComposeConfig, ComposeLogRequest, ComposeLogs, ComposeProject, ComposeServiceStatus,
    InfraError, InfraResult,
};

struct FakeCompose {
    statuses: Mutex<VecDeque<InfraResult<ComposeStatus>>>,
    actions: Mutex<Vec<ComposeMutationAction>>,
    failure: Option<MutationFailure>,
}

impl FakeCompose {
    fn with_statuses(statuses: impl IntoIterator<Item = ComposeStatus>) -> Self {
        Self {
            statuses: Mutex::new(statuses.into_iter().map(Ok).collect()),
            actions: Mutex::new(Vec::new()),
            failure: None,
        }
    }
}

#[async_trait]
impl ComposeInspector for FakeCompose {
    async fn list_projects(
        &self,
        _host: &HostRecord,
        _deadline: Timestamp,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ComposeProject>> {
        Ok(Vec::new())
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
                    message: "fixture exhausted".into(),
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
        unreachable!()
    }

    async fn logs(
        &self,
        _host: &HostRecord,
        _project: &ComposeProjectRef,
        _request: &ComposeLogRequest,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ComposeLogs> {
        unreachable!()
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

fn project() -> ComposeProjectRef {
    ComposeProjectRef::new("soma", "/srv/soma/compose.yaml").unwrap()
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

fn request(action: ComposeMutationAction) -> ComposeMutationRequest {
    ComposeMutationRequest::new(
        project(),
        action,
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
    )
}

fn engine() -> ComposeMutationEngine {
    ComposeMutationEngine::new(MutationVerificationPolicy::new(1, Duration::ZERO).unwrap())
}

#[tokio::test(flavor = "current_thread")]
async fn compose_up_and_restart_are_sent_and_verified() {
    for action in [ComposeMutationAction::Up, ComposeMutationAction::Restart] {
        let client = FakeCompose::with_statuses([
            status("exited", None),
            status("running", Some("healthy")),
        ]);
        let outcome = engine()
            .execute(
                &client,
                &host(),
                &request(action),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(outcome.action, action);
        assert_eq!(outcome.send_state, MutationSendState::Sent);
        assert_eq!(outcome.verification_status, VerificationStatus::Verified);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn unhealthy_or_nonrunning_services_fail_verification() {
    let client = FakeCompose::with_statuses([
        status("running", Some("healthy")),
        status("running", Some("unhealthy")),
    ]);
    let outcome = engine()
        .execute(
            &client,
            &host(),
            &request(ComposeMutationAction::Restart),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.send_state, MutationSendState::Sent);
    assert_eq!(outcome.verification_status, VerificationStatus::Failed);
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_before_compose_send_is_not_sent() {
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let failure = engine()
        .execute(
            &FakeCompose::with_statuses([]),
            &host(),
            &request(ComposeMutationAction::Up),
            &cancellation,
        )
        .await
        .unwrap_err();
    assert_eq!(failure.send_state(), MutationSendState::NotSent);
}
