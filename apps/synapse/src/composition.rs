use std::collections::BTreeMap;
use std::sync::Arc;

use soma_fleet::{ConnectionPool, HostId, OpenSshConnector, OpenSshDriver};
use soma_infra::{
    BollardClientProvider, BuildContextInspector, BuildContextPolicy, CommandBuildContextInspector,
    CommandComposeBuildMutator, CommandComposeInspector, CommandFileTransfer,
    CommandFilesystemQueryInspector, CommandHostExec, CommandHostSystemInspector,
    CommandImageBuildMutator, CommandLogReader, CommandProcessInspector, CommandZfsInspector,
    FileReadPolicy, FileTransferPolicy, FilesystemQueryInspector, HostExecPolicy,
    LinuxCommandHostInspector,
};
use synapse_application::{
    SynapseBuildPorts, SynapseCatalog, SynapseExecPorts, SynapseFinalPorts, SynapseMutationPorts,
    SynapseMutationRuntime, SynapseReadPorts, SynapseReadRuntime, SynapseRecreatePorts,
};

use crate::activity::ActivityLog;
use crate::config::SynapseConfig;
use crate::fleet::{
    PerHostBuildContext, PerHostFilesystem, RoutedCommandExecutor, StaticHostRepository,
};
use crate::{StandaloneError, StandaloneRuntime};

impl StandaloneRuntime {
    pub fn from_config(config: SynapseConfig) -> Result<Self, StandaloneError> {
        config.validate()?;
        let snapshot = crate::fleet::topology(&config)?;
        let repository = Arc::new(StaticHostRepository::new(snapshot));

        let connector = OpenSshConnector::default();
        let ssh = Arc::new(OpenSshDriver::new(connector.clone()));
        let executor = Arc::new(RoutedCommandExecutor::new(Arc::clone(&ssh)));
        let docker_pool = Arc::new(ConnectionPool::new(Arc::new(connector)));
        let mut docker = BollardClientProvider::new(docker_pool);
        for host in &config.hosts {
            if let Some(socket) = &host.docker_socket {
                docker = docker.with_remote_socket(HostId::new(&host.id)?, socket)?;
            }
        }
        let docker = Arc::new(docker);
        let compose = Arc::new(CommandComposeInspector::new(Arc::clone(&executor)));

        let mut filesystem_drivers = BTreeMap::<HostId, Arc<dyn FilesystemQueryInspector>>::new();
        let mut build_drivers = BTreeMap::<HostId, Arc<dyn BuildContextInspector>>::new();
        let mut host_exec = CommandHostExec::new(executor.clone());
        let mut transfer = CommandFileTransfer::new(executor.clone());
        for host in &config.hosts {
            let id = HostId::new(&host.id)?;
            let file_policy = FileReadPolicy::new(host.read_roots.clone())?;
            filesystem_drivers.insert(
                id.clone(),
                Arc::new(CommandFilesystemQueryInspector::new(
                    Arc::clone(&executor),
                    file_policy,
                )),
            );
            if !host.build_roots.is_empty() {
                build_drivers.insert(
                    id.clone(),
                    Arc::new(CommandBuildContextInspector::new(
                        Arc::clone(&executor),
                        BuildContextPolicy::new(host.build_roots.clone())?,
                    )),
                );
            }
            host_exec =
                host_exec.with_policy(id.clone(), HostExecPolicy::new(host.read_roots.clone())?);
            if !host.transfer_source_roots.is_empty() && !host.transfer_destination_roots.is_empty()
            {
                transfer = transfer.with_policy(
                    id,
                    FileTransferPolicy::new(
                        host.transfer_source_roots.clone(),
                        host.transfer_destination_roots.clone(),
                    )?,
                );
            }
        }
        let filesystem = Arc::new(PerHostFilesystem::new(filesystem_drivers));
        let contexts = Arc::new(PerHostBuildContext::new(build_drivers));
        let host_exec = Arc::new(host_exec);
        let transfer = Arc::new(transfer);

        let mut read = SynapseReadRuntime::new(SynapseReadPorts {
            hosts: repository.clone(),
            host: Arc::new(LinuxCommandHostInspector::new(Arc::clone(&executor))),
            host_system: Arc::new(CommandHostSystemInspector::new(Arc::clone(&executor))),
            docker: docker.clone(),
            compose: compose.clone(),
            filesystem: filesystem.clone(),
            processes: Arc::new(CommandProcessInspector::new(Arc::clone(&executor))),
            logs: Arc::new(CommandLogReader::new(Arc::clone(&executor))),
            zfs: Arc::new(CommandZfsInspector::new(Arc::clone(&executor))),
        })
        .with_timeout(config.server.request_timeout());
        if let Some(default) = &config.server.default_host {
            read = read.with_default_host(HostId::new(default)?);
        }

        let mutation = SynapseMutationRuntime::new(SynapseMutationPorts {
            hosts: repository,
            docker: docker.clone(),
            compose: Some(compose.clone()),
            artifacts: Some(docker.clone()),
            compose_pull: Some(compose.clone()),
            builds: Some(SynapseBuildPorts {
                contexts,
                image: Arc::new(CommandImageBuildMutator::new(Arc::clone(&executor))),
                compose: Arc::new(CommandComposeBuildMutator::new(Arc::clone(&executor))),
            }),
            recreate: Some(SynapseRecreatePorts {
                containers: docker.clone(),
                compose: compose.clone(),
            }),
            exec: Some(SynapseExecPorts {
                containers: docker.clone(),
                hosts: host_exec,
                max_fanout_concurrency: config.server.max_fanout_concurrency.clamp(1, 256),
            }),
            final_mutations: Some(SynapseFinalPorts {
                cleanup: docker,
                compose_down: compose,
                transfer,
            }),
        });

        Ok(Self {
            config,
            catalog: SynapseCatalog::embedded(),
            read,
            mutation,
            ssh,
            activity: ActivityLog::default(),
        })
    }
}

#[cfg(test)]
#[path = "composition_tests.rs"]
mod tests;
