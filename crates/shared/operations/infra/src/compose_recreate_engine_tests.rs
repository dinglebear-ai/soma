use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use async_trait::async_trait;
use soma_fleet::{HostEndpoint, HostId};
use soma_ops::{MutationSendState, OperationId, OperationName, Timestamp, VerificationStatus};

use super::*;
use crate::{
    ComposeConfig, ComposeLogRequest, ComposeLogs, ComposeProject, ComposeProjectRef,
    ComposeRecreateFingerprint, ComposeRecreateMutator, ComposeRecreateReceipt,
    ComposeServiceConfig, ComposeServiceStatus, ComposeStatus, InfraResult,
};

struct Fake {
    configs: Mutex<VecDeque<ComposeConfig>>,
    statuses: Mutex<VecDeque<ComposeStatus>>,
    mutate_count: Mutex<usize>,
}

#[async_trait]
impl crate::ComposeInspector for Fake {
    async fn list_projects(
        &self,
        _: &soma_fleet::HostRecord,
        _: Timestamp,
        _: &tokio_util::sync::CancellationToken,
    ) -> InfraResult<Vec<ComposeProject>> {
        Ok(Vec::new())
    }

    async fn status(
        &self,
        _: &soma_fleet::HostRecord,
        _: &ComposeProjectRef,
        _: Option<&str>,
        _: Timestamp,
        _: &tokio_util::sync::CancellationToken,
    ) -> InfraResult<ComposeStatus> {
        self.statuses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| crate::InfraError::Parse {
                domain: "test",
                message: "missing status".into(),
            })
    }

    async fn config(
        &self,
        _: &soma_fleet::HostRecord,
        _: &ComposeProjectRef,
        _: Timestamp,
        _: &tokio_util::sync::CancellationToken,
    ) -> InfraResult<ComposeConfig> {
        self.configs
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| crate::InfraError::Parse {
                domain: "test",
                message: "missing config".into(),
            })
    }

    async fn logs(
        &self,
        _: &soma_fleet::HostRecord,
        _: &ComposeProjectRef,
        _: &ComposeLogRequest,
        _: &tokio_util::sync::CancellationToken,
    ) -> InfraResult<ComposeLogs> {
        unreachable!()
    }
}

#[async_trait]
impl ComposeRecreateMutator for Fake {
    async fn recreate_compose(
        &self,
        host: &soma_fleet::HostRecord,
        request: &ComposeRecreateRequest,
        _: &tokio_util::sync::CancellationToken,
    ) -> MutationResult<ComposeRecreateReceipt> {
        *self.mutate_count.lock().unwrap() += 1;
        Ok(ComposeRecreateReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: request.project().name().into(),
            send_state: MutationSendState::Sent,
            stdout: String::new(),
            stderr: String::new(),
            output_truncated: false,
        })
    }
}

fn host() -> soma_fleet::HostRecord {
    soma_fleet::HostRecord::new(HostId::new("local").unwrap(), HostEndpoint::Local)
}

fn config(image: &str) -> ComposeConfig {
    ComposeConfig {
        host: host().id().clone(),
        topology_revision: host().revision().clone(),
        project: "soma".into(),
        services: BTreeMap::from([(
            "api".into(),
            ComposeServiceConfig {
                image: Some(image.into()),
                build_context: None,
                profiles: Vec::new(),
            },
        )]),
        networks: Vec::new(),
        volumes: Vec::new(),
    }
}

fn status(state: &str) -> ComposeStatus {
    ComposeStatus {
        host: host().id().clone(),
        topology_revision: host().revision().clone(),
        project: "soma".into(),
        services: vec![ComposeServiceStatus {
            service: "api".into(),
            container_name: Some("soma-api-1".into()),
            state: Some(state.into()),
            health: Some("healthy".into()),
            exit_code: Some(0),
            image: Some("api:v1".into()),
        }],
    }
}

fn request(expected: ComposeRecreateFingerprint) -> ComposeRecreateRequest {
    ComposeRecreateRequest::new(
        OperationId::new(),
        OperationName::new("compose.recreate").unwrap(),
        ComposeProjectRef::new("soma", "/srv/soma/compose.yaml").unwrap(),
        expected,
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
    )
}

#[tokio::test]
async fn compose_drift_rejects_before_force_recreate() {
    let before = status("running");
    let expected = compose_recreate_fingerprint(&config("api:v1"), &before).unwrap();
    let fake = Fake {
        configs: Mutex::new(VecDeque::from([config("api:v2")])),
        statuses: Mutex::new(VecDeque::from([before])),
        mutate_count: Mutex::new(0),
    };
    let error = ComposeRecreateEngine
        .execute(
            &fake,
            &host(),
            &request(expected),
            &tokio_util::sync::CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.send_state(), MutationSendState::NotSent);
    assert_eq!(*fake.mutate_count.lock().unwrap(), 0);
}

#[tokio::test]
async fn force_recreate_verifies_healthy_service_set() {
    let before = status("running");
    let cfg = config("api:v1");
    let expected = compose_recreate_fingerprint(&cfg, &before).unwrap();
    let fake = Fake {
        configs: Mutex::new(VecDeque::from([cfg])),
        statuses: Mutex::new(VecDeque::from([before, status("running")])),
        mutate_count: Mutex::new(0),
    };
    let outcome = ComposeRecreateEngine
        .execute(
            &fake,
            &host(),
            &request(expected),
            &tokio_util::sync::CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.verification_status, VerificationStatus::Verified);
}
