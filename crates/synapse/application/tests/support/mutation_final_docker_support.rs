use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use soma_fleet::{HostId, HostRecord};
use soma_infra::{
    ContainerInspect, ContainerListOptions, ContainerLogOptions, ContainerLogs,
    ContainerProcessTable, ContainerReader, ContainerStatsSnapshot, ContainerSummary,
    DockerCleanupClient, DockerCleanupClientProvider, DockerCleanupMutator, DockerDiskUsage,
    DockerPruneReceipt, DockerPruneRequest, DockerPruneScopeReceipt, DockerPruneTarget,
    DockerTelemetryReader, ImageListOptions, ImageReader, ImageRemovalReceipt, ImageRemovalRequest,
    ImageSummary, InfraError, InfraResult, MutationResult, NetworkReader, NetworkSummary,
    VolumeReader, VolumeSummary,
};
use soma_ops::MutationSendState;
use tokio_util::sync::CancellationToken;

pub(crate) struct FakeCleanup {
    image: ImageSummary,
    removed: Mutex<bool>,
    pruned: Mutex<bool>,
    remove_calls: Mutex<usize>,
    prune_calls: Mutex<usize>,
}

impl FakeCleanup {
    pub(crate) fn new(image: ImageSummary) -> Self {
        Self {
            image,
            removed: Mutex::new(false),
            pruned: Mutex::new(false),
            remove_calls: Mutex::new(0),
            prune_calls: Mutex::new(0),
        }
    }

    pub(crate) fn remove_calls(&self) -> usize {
        *self.remove_calls.lock().unwrap()
    }

    pub(crate) fn prune_calls(&self) -> usize {
        *self.prune_calls.lock().unwrap()
    }
}

#[async_trait]
impl ImageReader for FakeCleanup {
    async fn list_images(
        &self,
        _host: &HostRecord,
        options: &ImageListOptions,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ImageSummary>> {
        let absent = if options.dangling_only {
            *self.pruned.lock().unwrap()
        } else {
            *self.removed.lock().unwrap()
        };
        Ok((!absent).then(|| self.image.clone()).into_iter().collect())
    }
}

#[async_trait]
impl ContainerReader for FakeCleanup {
    async fn list_containers(
        &self,
        _host: &HostRecord,
        _options: &ContainerListOptions,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ContainerSummary>> {
        Ok(Vec::new())
    }

    async fn inspect_container(
        &self,
        _host: &HostRecord,
        _container: &str,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ContainerInspect> {
        Err(InfraError::Docker("unused cleanup inspect".into()))
    }

    async fn top_container(
        &self,
        _host: &HostRecord,
        _container: &str,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ContainerProcessTable> {
        Err(InfraError::Docker("unused cleanup top".into()))
    }
}

#[async_trait]
impl NetworkReader for FakeCleanup {
    async fn list_networks(
        &self,
        _host: &HostRecord,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Vec<NetworkSummary>> {
        Ok(Vec::new())
    }
}

#[async_trait]
impl VolumeReader for FakeCleanup {
    async fn list_volumes(
        &self,
        _host: &HostRecord,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Vec<VolumeSummary>> {
        Ok(Vec::new())
    }
}

#[async_trait]
impl DockerTelemetryReader for FakeCleanup {
    async fn disk_usage(
        &self,
        host: &HostRecord,
        _cancellation: &CancellationToken,
    ) -> InfraResult<DockerDiskUsage> {
        Ok(DockerDiskUsage {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            layers_size_bytes: 0,
            images: Default::default(),
            containers: Default::default(),
            volumes: Default::default(),
            build_cache: Default::default(),
        })
    }

    async fn container_logs(
        &self,
        _host: &HostRecord,
        _container: &str,
        _options: &ContainerLogOptions,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ContainerLogs> {
        Err(InfraError::Docker("unused cleanup logs".into()))
    }

    async fn container_stats(
        &self,
        _host: &HostRecord,
        _container: &str,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ContainerStatsSnapshot> {
        Err(InfraError::Docker("unused cleanup stats".into()))
    }
}

#[async_trait]
impl DockerCleanupMutator for FakeCleanup {
    async fn remove_image(
        &self,
        _host: &HostRecord,
        request: &ImageRemovalRequest,
        _cancellation: &CancellationToken,
    ) -> MutationResult<ImageRemovalReceipt> {
        *self.remove_calls.lock().unwrap() += 1;
        *self.removed.lock().unwrap() = true;
        Ok(ImageRemovalReceipt {
            send_state: MutationSendState::Sent,
            deleted: vec![request.fingerprint.identity.id.clone()],
            untagged: request.fingerprint.identity.repo_tags.clone(),
        })
    }

    async fn prune(
        &self,
        _host: &HostRecord,
        request: &DockerPruneRequest,
        _cancellation: &CancellationToken,
    ) -> MutationResult<DockerPruneReceipt> {
        *self.prune_calls.lock().unwrap() += 1;
        *self.pruned.lock().unwrap() = true;
        Ok(DockerPruneReceipt {
            send_state: MutationSendState::Sent,
            scopes: vec![DockerPruneScopeReceipt {
                target: DockerPruneTarget::Images,
                deleted: request.fingerprint.images.clone(),
                space_reclaimed: 1024,
            }],
        })
    }
}

pub(crate) struct FakeCleanupProvider(pub(crate) Arc<FakeCleanup>);

#[async_trait]
impl DockerCleanupClientProvider for FakeCleanupProvider {
    async fn cleanup_client(
        &self,
        _host: &HostRecord,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Arc<dyn DockerCleanupClient>> {
        Ok(self.0.clone())
    }
}

pub(crate) fn cleanup_image(host: &HostRecord) -> ImageSummary {
    ImageSummary {
        host: HostId::new(host.id().as_str()).unwrap(),
        topology_revision: host.revision().clone(),
        id: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        repo_tags: vec!["app:v1".into()],
        repo_digests: Vec::new(),
        created_unix_seconds: 0,
        size_bytes: 1024,
        containers: 0,
        labels: Default::default(),
    }
}
