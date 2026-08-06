use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use async_trait::async_trait;
use soma_fleet::{HostEndpoint, HostId};
use soma_ops::{NoopProgressSink, OperationId, OperationName};

use super::*;
use crate::{
    ComposeConfig, ComposeLogRequest, ComposeLogs, ComposeProject, ComposePullMutator,
    ComposePullReceipt, ComposeServiceConfig, ComposeStatus, ImageSummary, InfraResult,
};

struct FakeCompose {
    config: ComposeConfig,
    receipt: Mutex<Option<MutationResult<ComposePullReceipt>>>,
}

#[async_trait]
impl crate::ComposeInspector for FakeCompose {
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
        _project: &crate::ComposeProjectRef,
        _service: Option<&str>,
        _deadline: Timestamp,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ComposeStatus> {
        Err(InfraError::Docker("unused".into()))
    }

    async fn config(
        &self,
        _host: &HostRecord,
        _project: &crate::ComposeProjectRef,
        _deadline: Timestamp,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ComposeConfig> {
        Ok(self.config.clone())
    }

    async fn logs(
        &self,
        _host: &HostRecord,
        _project: &crate::ComposeProjectRef,
        _request: &ComposeLogRequest,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ComposeLogs> {
        Err(InfraError::Docker("unused".into()))
    }
}

#[async_trait]
impl ComposePullMutator for FakeCompose {
    async fn pull_compose_images(
        &self,
        _host: &HostRecord,
        _request: &ComposePullRequest,
        _progress: &dyn MutationProgressReporter,
        _cancellation: &CancellationToken,
    ) -> MutationResult<ComposePullReceipt> {
        self.receipt.lock().unwrap().take().unwrap()
    }
}

struct FakeImages {
    rows: Mutex<VecDeque<InfraResult<Vec<ImageSummary>>>>,
}

#[async_trait]
impl ImageReader for FakeImages {
    async fn list_images(
        &self,
        _host: &HostRecord,
        _options: &ImageListOptions,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ImageSummary>> {
        self.rows.lock().unwrap().pop_front().unwrap()
    }
}

fn host() -> HostRecord {
    HostRecord::new(HostId::new("devhost").unwrap(), HostEndpoint::Local)
}

fn project() -> crate::ComposeProjectRef {
    crate::ComposeProjectRef::new("soma", "/srv/soma/compose.yaml").unwrap()
}

fn request(service: Option<&str>) -> ComposePullRequest {
    ComposePullRequest::new(
        OperationId::new(),
        OperationName::new("compose.pull").unwrap(),
        project(),
        service.map(str::to_owned),
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
    )
    .unwrap()
}

fn config() -> ComposeConfig {
    ComposeConfig {
        host: HostId::new("devhost").unwrap(),
        topology_revision: host().revision().clone(),
        project: "soma".into(),
        services: BTreeMap::from([
            (
                "api".into(),
                ComposeServiceConfig {
                    image: Some("example/api:latest".into()),
                    build_context: None,
                    profiles: Vec::new(),
                },
            ),
            (
                "web".into(),
                ComposeServiceConfig {
                    image: Some("example/web:v1".into()),
                    build_context: None,
                    profiles: Vec::new(),
                },
            ),
        ]),
        networks: Vec::new(),
        volumes: Vec::new(),
    }
}

fn image(id: &str, tag: &str) -> ImageSummary {
    ImageSummary {
        host: HostId::new("devhost").unwrap(),
        topology_revision: host().revision().clone(),
        id: id.into(),
        repo_tags: vec![tag.into()],
        repo_digests: Vec::new(),
        created_unix_seconds: 0,
        size_bytes: 0,
        containers: 0,
        labels: BTreeMap::new(),
    }
}

fn receipt() -> ComposePullReceipt {
    ComposePullReceipt {
        host: HostId::new("devhost").unwrap(),
        topology_revision: host().revision().clone(),
        project: "soma".into(),
        service: None,
        send_state: MutationSendState::Sent,
        progress_delivery_errors: Vec::new(),
        output_truncated: false,
    }
}

#[tokio::test]
async fn compose_pull_verifies_each_configured_image() {
    let compose = FakeCompose {
        config: config(),
        receipt: Mutex::new(Some(Ok(receipt()))),
    };
    let images = FakeImages {
        rows: Mutex::new(VecDeque::from([
            Ok(vec![
                image("sha256:api-old", "example/api:latest"),
                image("sha256:web", "example/web:v1"),
            ]),
            Ok(vec![
                image("sha256:api-new", "example/api:latest"),
                image("sha256:web", "example/web:v1"),
            ]),
        ])),
    };
    let outcome = ComposePullEngine
        .execute(
            &compose,
            &images,
            &host(),
            &request(None),
            &NoopProgressSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.verification_status, VerificationStatus::Verified);
    assert!(outcome.changed);
    assert_eq!(outcome.images.len(), 2);
}

#[tokio::test]
async fn missing_image_after_pull_fails_verification() {
    let compose = FakeCompose {
        config: config(),
        receipt: Mutex::new(Some(Ok(receipt()))),
    };
    let images = FakeImages {
        rows: Mutex::new(VecDeque::from([Ok(Vec::new()), Ok(Vec::new())])),
    };
    let outcome = ComposePullEngine
        .execute(
            &compose,
            &images,
            &host(),
            &request(Some("api")),
            &NoopProgressSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(outcome.verification_status, VerificationStatus::Failed);
}

#[tokio::test]
async fn unknown_service_is_rejected_before_mutator_call() {
    let compose = FakeCompose {
        config: config(),
        receipt: Mutex::new(Some(Ok(receipt()))),
    };
    let images = FakeImages {
        rows: Mutex::new(VecDeque::new()),
    };
    let error = ComposePullEngine
        .execute(
            &compose,
            &images,
            &host(),
            &request(Some("missing")),
            &NoopProgressSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.send_state(), MutationSendState::NotSent);
}
