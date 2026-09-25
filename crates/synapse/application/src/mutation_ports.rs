use std::sync::Arc;

use soma_fleet::HostRepository;
use soma_infra::{
    BuildContextInspector, ComposeBuildMutator, ComposeDownClient, ComposeMutationClient,
    ComposePullClient, ComposeRecreateClient, ContainerExecClientProvider,
    ContainerRecreateClientProvider, DockerArtifactClientProvider, DockerCleanupClientProvider,
    DockerMutationClientProvider, HostExecMutator, ImageBuildMutator, VerifiedFileTransferClient,
};

/// Product-owned privileged build ports.
pub struct SynapseBuildPorts {
    /// Descriptor-confined build-context inspector.
    pub contexts: Arc<dyn BuildContextInspector>,
    /// Docker image build driver.
    pub image: Arc<dyn ImageBuildMutator>,
    /// Compose build driver.
    pub compose: Arc<dyn ComposeBuildMutator>,
}

/// Product-owned replacement ports.
pub struct SynapseRecreatePorts {
    /// Host-bound container replacement client provider.
    pub containers: Arc<dyn ContainerRecreateClientProvider>,
    /// Compose force-recreate client.
    pub compose: Arc<dyn ComposeRecreateClient>,
}

/// Product-owned bounded execution ports.
pub struct SynapseExecPorts {
    /// Host-bound Docker exec client provider.
    pub containers: Arc<dyn ContainerExecClientProvider>,
    /// Allowlisted descriptor-bound host command driver.
    pub hosts: Arc<dyn HostExecMutator>,
    /// Maximum in-flight fanout targets.
    pub max_fanout_concurrency: usize,
}

/// Product-owned ports for final cleanup and transfer mutations.
pub struct SynapseFinalPorts {
    /// Host-bound Docker cleanup client provider.
    pub cleanup: Arc<dyn DockerCleanupClientProvider>,
    /// Compose teardown client.
    pub compose_down: Arc<dyn ComposeDownClient>,
    /// Verified bounded file-transfer client.
    pub transfer: Arc<dyn VerifiedFileTransferClient>,
}

/// Product-owned ports used by canonical Synapse mutations.
pub struct SynapseMutationPorts {
    /// Fleet topology source.
    pub hosts: Arc<dyn HostRepository>,
    /// Host-bound Docker mutation client provider.
    pub docker: Arc<dyn DockerMutationClientProvider>,
    /// Optional Compose lifecycle mutation client.
    pub compose: Option<Arc<dyn ComposeMutationClient>>,
    /// Optional Docker artifact mutation client provider.
    pub artifacts: Option<Arc<dyn DockerArtifactClientProvider>>,
    /// Optional Compose artifact mutation client.
    pub compose_pull: Option<Arc<dyn ComposePullClient>>,
    /// Optional privileged build ports.
    pub builds: Option<SynapseBuildPorts>,
    /// Optional destructive replacement ports.
    pub recreate: Option<SynapseRecreatePorts>,
    /// Optional bounded execution ports.
    pub exec: Option<SynapseExecPorts>,
    /// Optional final cleanup and transfer ports.
    pub final_mutations: Option<SynapseFinalPorts>,
}

#[cfg(test)]
#[path = "mutation_ports_tests.rs"]
mod tests;
