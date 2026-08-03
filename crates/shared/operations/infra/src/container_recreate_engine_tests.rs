use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use async_trait::async_trait;
use soma_fleet::{HostEndpoint, HostId};
use soma_ops::{MutationSendState, OperationId, OperationName, Timestamp, VerificationStatus};

use super::*;
use crate::{
    ContainerInspect, ContainerListOptions, ContainerProcessTable, ContainerRecreateFingerprint,
    ContainerRecreateInspector, ContainerRecreateMutator, ContainerRecreateReceipt,
    ContainerSummary, InfraResult,
};

struct Fake {
    fingerprints: Mutex<VecDeque<ContainerRecreateFingerprint>>,
    inspections: Mutex<VecDeque<ContainerInspect>>,
    mutate_count: Mutex<usize>,
}

#[async_trait]
impl crate::ContainerReader for Fake {
    async fn list_containers(
        &self,
        _: &soma_fleet::HostRecord,
        _: &ContainerListOptions,
        _: &tokio_util::sync::CancellationToken,
    ) -> InfraResult<Vec<ContainerSummary>> {
        Ok(Vec::new())
    }

    async fn inspect_container(
        &self,
        _: &soma_fleet::HostRecord,
        _: &str,
        _: &tokio_util::sync::CancellationToken,
    ) -> InfraResult<ContainerInspect> {
        self.inspections
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| crate::InfraError::Parse {
                domain: "test",
                message: "missing inspection".into(),
            })
    }

    async fn top_container(
        &self,
        _: &soma_fleet::HostRecord,
        _: &str,
        _: &tokio_util::sync::CancellationToken,
    ) -> InfraResult<ContainerProcessTable> {
        unreachable!()
    }
}

#[async_trait]
impl ContainerRecreateInspector for Fake {
    async fn recreate_fingerprint(
        &self,
        _: &soma_fleet::HostRecord,
        _: &str,
        _: &tokio_util::sync::CancellationToken,
    ) -> InfraResult<ContainerRecreateFingerprint> {
        self.fingerprints
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| crate::InfraError::Parse {
                domain: "test",
                message: "missing fingerprint".into(),
            })
    }
}

#[async_trait]
impl ContainerRecreateMutator for Fake {
    async fn recreate_container(
        &self,
        host: &soma_fleet::HostRecord,
        request: &ContainerRecreateRequest,
        _: &tokio_util::sync::CancellationToken,
    ) -> MutationResult<ContainerRecreateReceipt> {
        *self.mutate_count.lock().unwrap() += 1;
        Ok(ContainerRecreateReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            original_container: request.expected().container.clone(),
            new_container: Some("new-id".into()),
            name: request.expected().name.clone(),
            image: request.expected().image.clone(),
            stage: ContainerRecreateStage::Started,
            send_state: MutationSendState::Sent,
            pulled: request.pull(),
        })
    }
}

fn host() -> soma_fleet::HostRecord {
    soma_fleet::HostRecord::new(HostId::new("local").unwrap(), HostEndpoint::Local)
}

fn fingerprint(value: &str) -> ContainerRecreateFingerprint {
    ContainerRecreateFingerprint::new(
        "old-id",
        "app",
        "app:v1",
        ContainerState::Running,
        value.repeat(64),
    )
    .unwrap()
}

fn inspect(id: &str, name: &str, state: ContainerState) -> ContainerInspect {
    ContainerInspect {
        host: host().id().clone(),
        topology_revision: host().revision().clone(),
        id: Some(id.into()),
        name: Some(name.into()),
        created: None,
        path: None,
        args: Vec::new(),
        image: Some("sha256:image".into()),
        state,
        pid: None,
        exit_code: None,
        restart_count: None,
        labels: BTreeMap::new(),
    }
}

fn request(expected: ContainerRecreateFingerprint) -> ContainerRecreateRequest {
    ContainerRecreateRequest::new(
        OperationId::new(),
        OperationName::new("container.recreate").unwrap(),
        expected,
        true,
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
    )
}

#[tokio::test]
async fn fingerprint_drift_rejects_before_mutation() {
    let fake = Fake {
        fingerprints: Mutex::new(VecDeque::from([fingerprint("b")])),
        inspections: Mutex::new(VecDeque::new()),
        mutate_count: Mutex::new(0),
    };
    let error = ContainerRecreateEngine
        .execute(
            &fake,
            &host(),
            &request(fingerprint("a")),
            &tokio_util::sync::CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.send_state(), MutationSendState::NotSent);
    assert_eq!(*fake.mutate_count.lock().unwrap(), 0);
}

#[tokio::test]
async fn replacement_is_verified_by_name_and_running_state() {
    let expected = fingerprint("a");
    let fake = Fake {
        fingerprints: Mutex::new(VecDeque::from([expected.clone()])),
        inspections: Mutex::new(VecDeque::from([
            inspect("old-id", "/app", ContainerState::Running),
            inspect("new-id", "/app", ContainerState::Running),
        ])),
        mutate_count: Mutex::new(0),
    };
    let outcome = ContainerRecreateEngine
        .execute(
            &fake,
            &host(),
            &request(expected),
            &tokio_util::sync::CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.verification_status, VerificationStatus::Verified);
    assert_eq!(outcome.new_container.as_deref(), Some("new-id"));
}
