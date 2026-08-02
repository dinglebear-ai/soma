use std::sync::Arc;

use async_trait::async_trait;
use soma_fleet::HostRecord;
use tokio_util::sync::CancellationToken;

use crate::{
    ContainerReader, DockerSystemReader, ImageReader, InfraResult, NetworkReader, VolumeReader,
};

/// Factory for host- and revision-bound Docker read clients.
#[async_trait]
pub trait DockerClientProvider: Send + Sync {
    /// Returns a client bound to the exact host revision.
    async fn client(
        &self,
        host: &HostRecord,
        cancellation: &CancellationToken,
    ) -> InfraResult<Arc<dyn DockerReadClient>>;
}

/// Complete neutral Docker read surface.
pub trait DockerReadClient:
    DockerSystemReader
    + ContainerReader
    + ImageReader
    + NetworkReader
    + VolumeReader
    + crate::DockerTelemetryReader
{
}

impl<T> DockerReadClient for T where
    T: DockerSystemReader
        + ContainerReader
        + ImageReader
        + NetworkReader
        + VolumeReader
        + crate::DockerTelemetryReader
{
}
