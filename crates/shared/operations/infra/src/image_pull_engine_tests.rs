use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use async_trait::async_trait;
use soma_fleet::{HostEndpoint, HostId};
use soma_ops::{NoopProgressSink, OperationId, OperationName, Timestamp};

use super::*;
use crate::{
    ContainerInspect, ContainerListOptions, ContainerProcessTable, ContainerReader,
    ContainerSummary, ImagePullMutator, ImagePullProgressFrame, ImagePullReceipt, ImageReader,
    InfraError, canonical_image_reference,
};

struct FakeClient {
    images: Mutex<VecDeque<Result<Vec<ImageSummary>, InfraError>>>,
    receipt: Mutex<Option<MutationResult<ImagePullReceipt>>>,
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
        _host: &HostRecord,
        _container: &str,
        _cancellation: &CancellationToken,
    ) -> crate::InfraResult<ContainerInspect> {
        Err(InfraError::Docker("unused".into()))
    }

    async fn top_container(
        &self,
        _host: &HostRecord,
        _container: &str,
        _cancellation: &CancellationToken,
    ) -> crate::InfraResult<ContainerProcessTable> {
        Err(InfraError::Docker("unused".into()))
    }
}

#[async_trait]
impl ImageReader for FakeClient {
    async fn list_images(
        &self,
        _host: &HostRecord,
        _options: &ImageListOptions,
        _cancellation: &CancellationToken,
    ) -> crate::InfraResult<Vec<ImageSummary>> {
        self.images
            .lock()
            .expect("images lock")
            .pop_front()
            .expect("image fixture")
    }
}

#[async_trait]
impl ImagePullMutator for FakeClient {
    async fn pull_image(
        &self,
        _host: &HostRecord,
        _request: &ImagePullRequest,
        _progress: &dyn MutationProgressReporter,
        _cancellation: &CancellationToken,
    ) -> MutationResult<ImagePullReceipt> {
        self.receipt
            .lock()
            .expect("receipt lock")
            .take()
            .expect("receipt fixture")
    }
}

fn host() -> HostRecord {
    HostRecord::new(HostId::new("devhost").unwrap(), HostEndpoint::Local)
}

fn image(id: &str, tag: &str) -> ImageSummary {
    ImageSummary {
        host: HostId::new("devhost").unwrap(),
        topology_revision: host().revision().clone(),
        id: id.into(),
        repo_tags: vec![tag.into()],
        repo_digests: vec![format!("{tag}@sha256:{}", "a".repeat(64))],
        created_unix_seconds: 1,
        size_bytes: 2,
        containers: 0,
        labels: BTreeMap::new(),
    }
}

fn request() -> ImagePullRequest {
    ImagePullRequest::new(
        OperationId::new(),
        OperationName::new("docker.pull").unwrap(),
        "alpine:latest",
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
    )
    .unwrap()
}

fn receipt() -> ImagePullReceipt {
    ImagePullReceipt {
        host: HostId::new("devhost").unwrap(),
        topology_revision: host().revision().clone(),
        image: "alpine:latest".into(),
        send_state: MutationSendState::Sent,
        total_events: 1,
        progress: vec![ImagePullProgressFrame {
            sequence: 1,
            status: Some("Downloaded newer image".into()),
            id: None,
            current: None,
            total: None,
            message: None,
            error: None,
        }],
        progress_truncated: false,
        progress_delivery_errors: Vec::new(),
    }
}

#[tokio::test]
async fn pull_is_verified_by_local_image_identity() {
    let client = FakeClient {
        images: Mutex::new(VecDeque::from([
            Ok(vec![image("sha256:old", "alpine:latest")]),
            Ok(vec![image("sha256:new", "alpine:latest")]),
        ])),
        receipt: Mutex::new(Some(Ok(receipt()))),
    };
    let outcome = ImagePullEngine
        .execute(
            &client,
            &host(),
            &request(),
            &NoopProgressSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(outcome.changed);
    assert_eq!(outcome.verification_status, VerificationStatus::Verified);
    assert_eq!(outcome.after.unwrap().id, "sha256:new");
}

#[tokio::test]
async fn completed_stream_without_local_image_is_failed_verification() {
    let client = FakeClient {
        images: Mutex::new(VecDeque::from([Ok(Vec::new()), Ok(Vec::new())])),
        receipt: Mutex::new(Some(Ok(receipt()))),
    };
    let outcome = ImagePullEngine
        .execute(
            &client,
            &host(),
            &request(),
            &NoopProgressSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.send_state, MutationSendState::Sent);
    assert_eq!(outcome.verification_status, VerificationStatus::Failed);
}

#[tokio::test]
async fn verification_read_failure_is_inconclusive_not_a_fake_driver_failure() {
    let client = FakeClient {
        images: Mutex::new(VecDeque::from([
            Ok(Vec::new()),
            Err(InfraError::Docker("socket reset".into())),
        ])),
        receipt: Mutex::new(Some(Ok(receipt()))),
    };
    let outcome = ImagePullEngine
        .execute(
            &client,
            &host(),
            &request(),
            &NoopProgressSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(
        outcome.verification_status,
        VerificationStatus::Inconclusive
    );
}

#[test]
fn image_reference_defaults_only_the_tag_after_the_last_slash() {
    assert_eq!(canonical_image_reference("alpine"), "alpine:latest");
    assert_eq!(
        canonical_image_reference("registry:5000/repo"),
        "registry:5000/repo:latest"
    );
    assert_eq!(canonical_image_reference("repo:v1"), "repo:v1");
}
