use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use soma_fleet::{HostEndpoint, HostId, HostRecord, TopologySnapshot};
use soma_infra::{
    ContainerExecClientProvider, ContainerExecMutator, ContainerExecReceipt, ContainerExecRequest,
    HostExecMutator, HostExecReceipt, HostExecRequest, InfraError, InfraResult, MutationResult,
};
use soma_ops::{MutationSendState, OperationName};
use tokio_util::sync::CancellationToken;

use crate::mutation_pull_test_support::{StaticHosts, UnusedLifecycle};
use crate::{SynapseExecPorts, SynapseMutationPorts, SynapseMutationRuntime};

pub(crate) struct FakeContainerExec {
    pub(crate) receipts: Mutex<VecDeque<MutationResult<ContainerExecReceipt>>>,
    pub(crate) calls: Mutex<usize>,
}

#[async_trait]
impl ContainerExecMutator for FakeContainerExec {
    async fn exec_container(
        &self,
        _host: &HostRecord,
        _request: &ContainerExecRequest,
        _cancellation: &CancellationToken,
    ) -> MutationResult<ContainerExecReceipt> {
        *self.calls.lock().unwrap() += 1;
        self.receipts.lock().unwrap().pop_front().unwrap()
    }
}

pub(crate) struct FakeContainerExecProvider(pub(crate) Arc<FakeContainerExec>);

#[async_trait]
impl ContainerExecClientProvider for FakeContainerExecProvider {
    async fn exec_client(
        &self,
        _host: &HostRecord,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Arc<dyn ContainerExecMutator>> {
        Ok(self.0.clone())
    }
}

pub(crate) struct FakeHostExec {
    pub(crate) calls: Mutex<Vec<String>>,
}

#[async_trait]
impl HostExecMutator for FakeHostExec {
    async fn exec_host(
        &self,
        host: &HostRecord,
        request: &HostExecRequest,
        _cancellation: &CancellationToken,
    ) -> MutationResult<HostExecReceipt> {
        self.calls.lock().unwrap().push(host.id().to_string());
        if host.id().as_str() == "lost" {
            return Err(soma_infra::MutationFailure::new(
                MutationSendState::Unknown,
                InfraError::Fleet(soma_fleet::FleetError::RemoteCommandDetached {
                    host: host.id().clone(),
                    reason: "connection",
                }),
            ));
        }
        let exit_code = if host.id().as_str() == "bad" { 2 } else { 0 };
        Ok(HostExecReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            command: request.command(),
            args: request.args().to_vec(),
            working_dir: request.working_dir().map(ToOwned::to_owned),
            stdout: host.id().to_string(),
            stderr: if exit_code == 0 {
                String::new()
            } else {
                "failed".into()
            },
            exit_code: Some(exit_code),
            truncated: false,
            encoding_lossy: false,
            send_state: MutationSendState::Sent,
        })
    }
}

pub(crate) fn host(name: &str) -> HostRecord {
    HostRecord::new(HostId::new(name).unwrap(), HostEndpoint::Local)
}

pub(crate) fn container_receipt(exit_code: i64) -> ContainerExecReceipt {
    ContainerExecReceipt {
        host: HostId::new("devhost").unwrap(),
        topology_revision: host("devhost").revision().clone(),
        container: "api".into(),
        command: vec!["printf".into(), "ok".into()],
        user: None,
        working_dir: Some("/app".into()),
        stdout: "ok".into(),
        stderr: String::new(),
        exit_code: Some(exit_code),
        truncated: false,
        encoding_lossy: false,
        send_state: MutationSendState::Sent,
    }
}

pub(crate) fn runtime(
    container: Arc<FakeContainerExec>,
    hosts: Arc<FakeHostExec>,
    enabled: bool,
) -> SynapseMutationRuntime {
    let snapshot =
        TopologySnapshot::new([host("devhost"), host("alpha"), host("bad"), host("lost")]).unwrap();
    SynapseMutationRuntime::new(SynapseMutationPorts {
        hosts: Arc::new(StaticHosts(snapshot)),
        docker: Arc::new(UnusedLifecycle),
        compose: None,
        artifacts: None,
        compose_pull: None,
        builds: None,
        recreate: None,
        exec: enabled.then(|| SynapseExecPorts {
            containers: Arc::new(FakeContainerExecProvider(container)),
            hosts,
            max_fanout_concurrency: 2,
        }),
    })
}

pub(crate) fn op(name: &str) -> OperationName {
    OperationName::new(name).unwrap()
}
