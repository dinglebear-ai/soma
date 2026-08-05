use std::collections::{BTreeMap, VecDeque};
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use soma_fleet::{FleetResult, HostEndpoint, HostId, HostRecord, HostRepository, TopologySnapshot};
use soma_infra::{
    ComposePullClient, ContainerInspect, ContainerListOptions, ContainerProcessTable,
    ContainerReader, ContainerSummary, DockerArtifactClient, DockerArtifactClientProvider,
    DockerMutationClient, DockerMutationClientProvider, ImageListOptions, ImagePullMutator,
    ImagePullProgressFrame, ImagePullReceipt, ImagePullRequest, ImageReader, ImageSummary,
    InfraError, InfraResult, MutationProgressReporter, MutationResult,
};
use soma_ops::{
    AuthorizationEvidence, AuthorizationScope, IdempotencyKey, MutationSendState, OperationContext,
    OperationName, ProducerRef, ProgressEvent, ProgressSink, Timestamp,
};
use tokio_util::sync::CancellationToken;

pub(crate) use crate::mutation_pull_test_compose::{FakeComposePull, compose_config};
use crate::{SynapseMutationPorts, SynapseMutationRuntime};

pub(crate) struct StaticHosts(pub(crate) TopologySnapshot);

#[async_trait]
impl HostRepository for StaticHosts {
    async fn snapshot(&self) -> FleetResult<TopologySnapshot> {
        Ok(self.0.clone())
    }
}

pub(crate) struct UnusedLifecycle;

#[async_trait]
impl DockerMutationClientProvider for UnusedLifecycle {
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

pub(crate) struct FakeArtifacts {
    pub(crate) containers: Mutex<VecDeque<InfraResult<Vec<ContainerSummary>>>>,
    pub(crate) images: Mutex<VecDeque<InfraResult<Vec<ImageSummary>>>>,
    pub(crate) pull_count: Mutex<usize>,
    pub(crate) progress_delivery_error: bool,
}

#[async_trait]
impl ContainerReader for FakeArtifacts {
    async fn list_containers(
        &self,
        _host: &HostRecord,
        _options: &ContainerListOptions,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ContainerSummary>> {
        self.containers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Ok(Vec::new()))
    }

    async fn inspect_container(
        &self,
        _host: &HostRecord,
        _container: &str,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ContainerInspect> {
        Err(InfraError::Docker("unused".into()))
    }

    async fn top_container(
        &self,
        _host: &HostRecord,
        _container: &str,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ContainerProcessTable> {
        Err(InfraError::Docker("unused".into()))
    }
}

#[async_trait]
impl ImageReader for FakeArtifacts {
    async fn list_images(
        &self,
        _host: &HostRecord,
        _options: &ImageListOptions,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ImageSummary>> {
        self.images
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Ok(Vec::new()))
    }
}

#[async_trait]
impl ImagePullMutator for FakeArtifacts {
    async fn pull_image(
        &self,
        host: &HostRecord,
        request: &ImagePullRequest,
        progress: &dyn MutationProgressReporter,
        _cancellation: &CancellationToken,
    ) -> MutationResult<ImagePullReceipt> {
        *self.pull_count.lock().unwrap() += 1;
        let event = ProgressEvent::new(
            request.operation_id().clone(),
            request.operation().clone(),
            1,
            Timestamp::now(),
            "pull",
        )
        .unwrap()
        .with_message("downloaded image layers")
        .unwrap();
        let mut errors = Vec::new();
        if self.progress_delivery_error {
            if let Err(error) = progress.report(&event) {
                errors.push(error);
            }
        } else {
            progress.report(&event).unwrap();
        }
        Ok(ImagePullReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            image: request.image().into(),
            send_state: MutationSendState::Sent,
            total_events: 1,
            progress: vec![ImagePullProgressFrame {
                sequence: 1,
                status: Some("complete".into()),
                id: None,
                current: None,
                total: None,
                message: Some("downloaded image layers".into()),
                error: None,
            }],
            progress_truncated: false,
            progress_delivery_errors: errors,
        })
    }
}

pub(crate) struct FakeArtifactProvider(pub(crate) Arc<FakeArtifacts>);

#[async_trait]
impl DockerArtifactClientProvider for FakeArtifactProvider {
    async fn artifact_client(
        &self,
        _host: &HostRecord,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Arc<dyn DockerArtifactClient>> {
        Ok(self.0.clone())
    }
}

pub(crate) struct CollectProgress(pub(crate) Mutex<Vec<ProgressEvent>>);

impl ProgressSink for CollectProgress {
    type Error = Infallible;

    fn report(&self, event: &ProgressEvent) -> Result<(), Self::Error> {
        self.0.lock().unwrap().push(event.clone());
        Ok(())
    }
}

pub(crate) struct FailingProgress;

impl ProgressSink for FailingProgress {
    type Error = &'static str;

    fn report(&self, _event: &ProgressEvent) -> Result<(), Self::Error> {
        Err("progress sink offline")
    }
}

pub(crate) fn host() -> HostRecord {
    HostRecord::new(HostId::new("devhost").unwrap(), HostEndpoint::Local)
}

pub(crate) fn context() -> OperationContext {
    OperationContext::new()
        .with_idempotency_key(IdempotencyKey::new("pull-request").unwrap())
        .with_deadline(Timestamp::from_unix_millis(
            Timestamp::now().unix_millis() + 20_000,
        ))
}

pub(crate) fn authorization(
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
    .with_confirmation_ref("pull-confirmation")
    .unwrap()
}

pub(crate) fn runtime(
    artifacts: Option<Arc<FakeArtifacts>>,
    compose: Option<Arc<FakeComposePull>>,
) -> SynapseMutationRuntime {
    SynapseMutationRuntime::new(SynapseMutationPorts {
        hosts: Arc::new(StaticHosts(TopologySnapshot::new([host()]).unwrap())),
        docker: Arc::new(UnusedLifecycle),
        compose: None,
        artifacts: artifacts.map(|client| {
            Arc::new(FakeArtifactProvider(client)) as Arc<dyn DockerArtifactClientProvider>
        }),
        compose_pull: compose.map(|client| client as Arc<dyn ComposePullClient>),
        builds: None,
        recreate: None,
    })
}

pub(crate) fn image(id: &str, tag: &str) -> ImageSummary {
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

pub(crate) fn container(id: &str, image: &str) -> ContainerSummary {
    ContainerSummary {
        host: HostId::new("devhost").unwrap(),
        topology_revision: host().revision().clone(),
        id: Some(id.into()),
        names: vec![id.into()],
        image: Some(image.into()),
        image_id: None,
        command: None,
        created_unix_seconds: None,
        state: soma_infra::ContainerState::Running,
        status: None,
        labels: BTreeMap::new(),
    }
}
