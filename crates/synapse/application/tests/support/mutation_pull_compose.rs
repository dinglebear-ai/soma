use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use soma_fleet::{HostId, HostRecord};
use soma_infra::{
    ComposeConfig, ComposeInspector, ComposeLogRequest, ComposeLogs, ComposeProject,
    ComposeProjectRef, ComposePullMutator, ComposePullReceipt, ComposePullRequest,
    ComposeServiceConfig, ComposeStatus, InfraError, InfraResult, MutationProgressReporter,
    MutationResult,
};
use soma_ops::{MutationSendState, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::mutation_pull_test_support::host;

pub(crate) struct FakeComposePull {
    pub(crate) config: ComposeConfig,
    pub(crate) pull_count: Mutex<usize>,
}

#[async_trait]
impl ComposeInspector for FakeComposePull {
    async fn list_projects(
        &self,
        host: &HostRecord,
        _deadline: Timestamp,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ComposeProject>> {
        Ok(vec![ComposeProject {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            name: "soma".into(),
            status: None,
            config_files: vec!["/srv/soma/compose.yaml".into()],
        }])
    }

    async fn status(
        &self,
        _host: &HostRecord,
        _project: &ComposeProjectRef,
        _service: Option<&str>,
        _deadline: Timestamp,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ComposeStatus> {
        Err(InfraError::Docker("unused".into()))
    }

    async fn config(
        &self,
        _host: &HostRecord,
        _project: &ComposeProjectRef,
        _deadline: Timestamp,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ComposeConfig> {
        Ok(self.config.clone())
    }

    async fn logs(
        &self,
        _host: &HostRecord,
        _project: &ComposeProjectRef,
        _request: &ComposeLogRequest,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ComposeLogs> {
        Err(InfraError::Docker("unused".into()))
    }
}

#[async_trait]
impl ComposePullMutator for FakeComposePull {
    async fn pull_compose_images(
        &self,
        host: &HostRecord,
        request: &ComposePullRequest,
        _progress: &dyn MutationProgressReporter,
        _cancellation: &CancellationToken,
    ) -> MutationResult<ComposePullReceipt> {
        *self.pull_count.lock().unwrap() += 1;
        Ok(ComposePullReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: request.project().name().into(),
            service: request.service().map(str::to_owned),
            send_state: MutationSendState::Sent,
            progress_delivery_errors: Vec::new(),
            output_truncated: false,
        })
    }
}

pub(crate) fn compose_config() -> ComposeConfig {
    ComposeConfig {
        host: HostId::new("dookie").unwrap(),
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
