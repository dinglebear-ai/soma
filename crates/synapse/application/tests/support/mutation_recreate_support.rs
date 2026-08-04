use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use soma_fleet::{HostRecord, TopologySnapshot};
use soma_infra::{
    ComposeConfig, ComposeInspector, ComposeLogRequest, ComposeLogs, ComposeProject,
    ComposeProjectRef, ComposeRecreateClient, ComposeRecreateMutator, ComposeRecreateReceipt,
    ComposeRecreateRequest, ComposeServiceConfig, ComposeServiceStatus, ComposeStatus,
    ContainerInspect, ContainerListOptions, ContainerProcessTable, ContainerRecreateClient,
    ContainerRecreateClientProvider, ContainerRecreateFingerprint, ContainerRecreateInspector,
    ContainerRecreateMutator, ContainerRecreateReceipt, ContainerRecreateRequest,
    ContainerRecreateStage, ContainerState, ContainerSummary, InfraResult, MutationResult,
};
use soma_ops::{MutationSendState, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::mutation_pull_test_support::{StaticHosts, UnusedLifecycle, host};
use crate::{SynapseMutationPorts, SynapseMutationRuntime, SynapseRecreatePorts};

pub(crate) struct FakeContainerRecreate {
    pub fingerprints: Mutex<VecDeque<ContainerRecreateFingerprint>>,
    pub inspections: Mutex<VecDeque<ContainerInspect>>,
    pub mutations: Mutex<usize>,
}

#[async_trait]
impl soma_infra::ContainerReader for FakeContainerRecreate {
    async fn list_containers(
        &self,
        _: &HostRecord,
        _: &ContainerListOptions,
        _: &CancellationToken,
    ) -> InfraResult<Vec<ContainerSummary>> {
        Ok(Vec::new())
    }

    async fn inspect_container(
        &self,
        _: &HostRecord,
        _: &str,
        _: &CancellationToken,
    ) -> InfraResult<ContainerInspect> {
        self.inspections
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| soma_infra::InfraError::Parse {
                domain: "test",
                message: "missing container inspection".into(),
            })
    }

    async fn top_container(
        &self,
        _: &HostRecord,
        _: &str,
        _: &CancellationToken,
    ) -> InfraResult<ContainerProcessTable> {
        unreachable!()
    }
}

#[async_trait]
impl ContainerRecreateInspector for FakeContainerRecreate {
    async fn recreate_fingerprint(
        &self,
        _: &HostRecord,
        _: &str,
        _: &CancellationToken,
    ) -> InfraResult<ContainerRecreateFingerprint> {
        self.fingerprints
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| soma_infra::InfraError::Parse {
                domain: "test",
                message: "missing container fingerprint".into(),
            })
    }
}

#[async_trait]
impl ContainerRecreateMutator for FakeContainerRecreate {
    async fn recreate_container(
        &self,
        host: &HostRecord,
        request: &ContainerRecreateRequest,
        _: &CancellationToken,
    ) -> MutationResult<ContainerRecreateReceipt> {
        *self.mutations.lock().unwrap() += 1;
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

pub(crate) struct FakeContainerProvider(pub Arc<FakeContainerRecreate>);
#[async_trait]
impl ContainerRecreateClientProvider for FakeContainerProvider {
    async fn recreate_client(
        &self,
        _: &HostRecord,
        _: &CancellationToken,
    ) -> InfraResult<Arc<dyn ContainerRecreateClient>> {
        Ok(self.0.clone())
    }
}

pub(crate) struct FakeComposeRecreate {
    pub projects: Mutex<VecDeque<Vec<ComposeProject>>>,
    pub configs: Mutex<VecDeque<ComposeConfig>>,
    pub statuses: Mutex<VecDeque<ComposeStatus>>,
    pub mutations: Mutex<usize>,
}

#[async_trait]
impl ComposeInspector for FakeComposeRecreate {
    async fn list_projects(
        &self,
        _: &HostRecord,
        _: Timestamp,
        _: &CancellationToken,
    ) -> InfraResult<Vec<ComposeProject>> {
        self.projects
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| soma_infra::InfraError::Parse {
                domain: "test",
                message: "missing project list".into(),
            })
    }

    async fn status(
        &self,
        _: &HostRecord,
        _: &ComposeProjectRef,
        _: Option<&str>,
        _: Timestamp,
        _: &CancellationToken,
    ) -> InfraResult<ComposeStatus> {
        self.statuses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| soma_infra::InfraError::Parse {
                domain: "test",
                message: "missing Compose status".into(),
            })
    }

    async fn config(
        &self,
        _: &HostRecord,
        _: &ComposeProjectRef,
        _: Timestamp,
        _: &CancellationToken,
    ) -> InfraResult<ComposeConfig> {
        self.configs
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| soma_infra::InfraError::Parse {
                domain: "test",
                message: "missing Compose config".into(),
            })
    }

    async fn logs(
        &self,
        _: &HostRecord,
        _: &ComposeProjectRef,
        _: &ComposeLogRequest,
        _: &CancellationToken,
    ) -> InfraResult<ComposeLogs> {
        unreachable!()
    }
}

#[async_trait]
impl ComposeRecreateMutator for FakeComposeRecreate {
    async fn recreate_compose(
        &self,
        host: &HostRecord,
        request: &ComposeRecreateRequest,
        _: &CancellationToken,
    ) -> MutationResult<ComposeRecreateReceipt> {
        *self.mutations.lock().unwrap() += 1;
        Ok(ComposeRecreateReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: request.project().name().into(),
            send_state: MutationSendState::Sent,
            stdout: "recreated".into(),
            stderr: String::new(),
            output_truncated: false,
        })
    }
}

pub(crate) fn fingerprint(value: &str) -> ContainerRecreateFingerprint {
    ContainerRecreateFingerprint::new(
        "old-id",
        "app",
        "app:v1",
        ContainerState::Running,
        value.repeat(64),
    )
    .unwrap()
}

pub(crate) fn container_inspect(id: &str, state: ContainerState) -> ContainerInspect {
    ContainerInspect {
        host: host().id().clone(),
        topology_revision: host().revision().clone(),
        id: Some(id.into()),
        name: Some("/app".into()),
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

pub(crate) fn compose_config(image: &str) -> ComposeConfig {
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

pub(crate) fn compose_status(state: &str) -> ComposeStatus {
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

pub(crate) fn projects() -> Vec<ComposeProject> {
    vec![ComposeProject {
        host: host().id().clone(),
        topology_revision: host().revision().clone(),
        name: "soma".into(),
        status: Some("running".into()),
        config_files: vec!["/srv/soma/compose.yaml".into()],
    }]
}

pub(crate) fn runtime(
    container: Arc<FakeContainerRecreate>,
    compose: Arc<FakeComposeRecreate>,
) -> SynapseMutationRuntime {
    SynapseMutationRuntime::new(SynapseMutationPorts {
        hosts: Arc::new(StaticHosts(TopologySnapshot::new([host()]).unwrap())),
        docker: Arc::new(UnusedLifecycle),
        compose: None,
        artifacts: None,
        compose_pull: None,
        builds: None,
        recreate: Some(SynapseRecreatePorts {
            containers: Arc::new(FakeContainerProvider(container)),
            compose: compose as Arc<dyn ComposeRecreateClient>,
        }),
        exec: None,
    })
}
