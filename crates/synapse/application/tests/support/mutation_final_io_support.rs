use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use soma_fleet::{
    FileTransfer, FleetResult, HostEndpoint, HostId, HostRecord, TopologySnapshot, TransferReceipt,
    TransferRequest,
};
use soma_infra::{
    ComposeConfig, ComposeDownClient, ComposeDownMutator, ComposeDownReceipt, ComposeInspector,
    ComposeLogRequest, ComposeLogs, ComposeProject, ComposeProjectRef, ComposeStatus,
    FileTransferInspector, FileTransferPathRole, InfraError, InfraResult, MutationResult,
    TransferFileIdentity, VerifiedFileTransferClient,
};
use soma_ops::{MutationSendState, OperationName, Timestamp};
use tokio_util::sync::CancellationToken;

use crate::mutation_final_test_docker::{FakeCleanup, FakeCleanupProvider};
use crate::mutation_pull_test_support::{StaticHosts, UnusedLifecycle};
use crate::{SynapseFinalPorts, SynapseMutationPorts, SynapseMutationRuntime};

pub(crate) struct FakeComposeDown {
    down: Mutex<bool>,
    calls: Mutex<usize>,
}

impl FakeComposeDown {
    pub(crate) fn new() -> Self {
        Self {
            down: Mutex::new(false),
            calls: Mutex::new(0),
        }
    }

    pub(crate) fn calls(&self) -> usize {
        *self.calls.lock().unwrap()
    }
}

#[async_trait]
impl ComposeInspector for FakeComposeDown {
    async fn list_projects(
        &self,
        _host: &HostRecord,
        _deadline: Timestamp,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ComposeProject>> {
        Ok(crate::mutation_recreate_test_support::projects())
    }

    async fn status(
        &self,
        _host: &HostRecord,
        _project: &ComposeProjectRef,
        _service: Option<&str>,
        _deadline: Timestamp,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ComposeStatus> {
        if *self.down.lock().unwrap() {
            let mut status = crate::mutation_recreate_test_support::compose_status("exited");
            status.services.clear();
            Ok(status)
        } else {
            Ok(crate::mutation_recreate_test_support::compose_status(
                "running",
            ))
        }
    }

    async fn config(
        &self,
        _host: &HostRecord,
        _project: &ComposeProjectRef,
        _deadline: Timestamp,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ComposeConfig> {
        Ok(crate::mutation_recreate_test_support::compose_config(
            "api:v1",
        ))
    }

    async fn logs(
        &self,
        _host: &HostRecord,
        _project: &ComposeProjectRef,
        _request: &ComposeLogRequest,
        _cancellation: &CancellationToken,
    ) -> InfraResult<ComposeLogs> {
        Err(InfraError::InvalidRequest {
            domain: "compose-down",
            message: "unused final logs".into(),
        })
    }
}

#[async_trait]
impl ComposeDownMutator for FakeComposeDown {
    async fn down_compose(
        &self,
        host: &HostRecord,
        request: &soma_infra::ComposeDownRequest,
        _cancellation: &CancellationToken,
    ) -> MutationResult<ComposeDownReceipt> {
        *self.calls.lock().unwrap() += 1;
        *self.down.lock().unwrap() = true;
        Ok(ComposeDownReceipt {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: request.project().name().into(),
            remove_volumes: request.remove_volumes(),
            send_state: MutationSendState::Sent,
            stdout: "removed".into(),
            stderr: String::new(),
            output_truncated: false,
        })
    }
}

pub(crate) struct FakeTransfer {
    source: TransferFileIdentity,
    destination_before: Option<TransferFileIdentity>,
    transferred: Mutex<bool>,
    calls: Mutex<usize>,
}

impl FakeTransfer {
    pub(crate) fn new() -> Self {
        Self {
            source: TransferFileIdentity {
                path: "/source/payload.bin".into(),
                bytes: 7,
                sha256: "a".repeat(64),
            },
            destination_before: None,
            transferred: Mutex::new(false),
            calls: Mutex::new(0),
        }
    }

    pub(crate) fn calls(&self) -> usize {
        *self.calls.lock().unwrap()
    }
}

#[async_trait]
impl FileTransferInspector for FakeTransfer {
    async fn inspect_transfer_file(
        &self,
        host: &HostRecord,
        path: &Path,
        role: FileTransferPathRole,
        _optional: bool,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Option<TransferFileIdentity>> {
        match role {
            FileTransferPathRole::Source if host.id().as_str() == "source" => {
                Ok(Some(self.source.clone()))
            }
            FileTransferPathRole::Destination if host.id().as_str() == "destination" => {
                if *self.transferred.lock().unwrap() {
                    Ok(Some(TransferFileIdentity {
                        path: path.to_path_buf(),
                        bytes: self.source.bytes,
                        sha256: self.source.sha256.clone(),
                    }))
                } else {
                    Ok(self.destination_before.clone())
                }
            }
            _ => Err(InfraError::InvalidRequest {
                domain: "file-transfer",
                message: "unexpected fake transfer endpoint".into(),
            }),
        }
    }
}

#[async_trait]
impl FileTransfer for FakeTransfer {
    async fn transfer(
        &self,
        _source: &HostRecord,
        _destination: &HostRecord,
        _request: &TransferRequest,
        _cancellation: &CancellationToken,
    ) -> FleetResult<TransferReceipt> {
        *self.calls.lock().unwrap() += 1;
        *self.transferred.lock().unwrap() = true;
        TransferReceipt::new(self.source.bytes)
            .with_digests(self.source.sha256.clone(), self.source.sha256.clone())
            .map_err(Into::into)
    }
}

pub(crate) fn source_host() -> HostRecord {
    HostRecord::new(HostId::new("source").unwrap(), HostEndpoint::Local)
}

pub(crate) fn destination_host() -> HostRecord {
    HostRecord::new(HostId::new("destination").unwrap(), HostEndpoint::Local)
}

pub(crate) fn runtime(
    cleanup: Arc<FakeCleanup>,
    compose: Arc<FakeComposeDown>,
    transfer: Arc<FakeTransfer>,
    enabled: bool,
) -> SynapseMutationRuntime {
    let devhost = HostRecord::new(HostId::new("devhost").unwrap(), HostEndpoint::Local);
    SynapseMutationRuntime::new(SynapseMutationPorts {
        hosts: Arc::new(StaticHosts(
            TopologySnapshot::new([devhost, source_host(), destination_host()]).unwrap(),
        )),
        docker: Arc::new(UnusedLifecycle),
        compose: None,
        artifacts: None,
        compose_pull: None,
        builds: None,
        recreate: None,
        exec: None,
        final_mutations: enabled.then(|| SynapseFinalPorts {
            cleanup: Arc::new(FakeCleanupProvider(cleanup)),
            compose_down: compose as Arc<dyn ComposeDownClient>,
            transfer: transfer as Arc<dyn VerifiedFileTransferClient>,
        }),
    })
}

pub(crate) fn op(name: &str) -> OperationName {
    OperationName::new(name).unwrap()
}
