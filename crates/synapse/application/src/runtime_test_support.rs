use std::sync::Arc;

use async_trait::async_trait;
use soma_fleet::{FleetResult, HostEndpoint, HostId, HostRecord, HostRepository, TopologySnapshot};
use soma_infra::*;
use soma_ops::Timestamp;
use tokio_util::sync::CancellationToken;

use crate::runtime_test_docker::MockDocker;
use crate::{SynapseReadPorts, SynapseReadRuntime};

pub(crate) fn runtime() -> SynapseReadRuntime {
    let fixture = Arc::new(Fixture);
    SynapseReadRuntime::new(SynapseReadPorts {
        hosts: fixture.clone(),
        host: fixture.clone(),
        host_system: fixture.clone(),
        docker: fixture.clone(),
        compose: fixture.clone(),
        filesystem: fixture.clone(),
        processes: fixture.clone(),
        logs: fixture.clone(),
        zfs: fixture,
    })
}

fn host() -> HostRecord {
    HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local)
}

pub(crate) struct Fixture;

#[async_trait]
impl HostRepository for Fixture {
    async fn snapshot(&self) -> FleetResult<TopologySnapshot> {
        Ok(TopologySnapshot::new([host()]).unwrap())
    }
}

#[async_trait]
impl HostInspector for Fixture {
    async fn inspect(
        &self,
        host: &HostRecord,
        _request: HostInspectRequest,
        _cancellation: &CancellationToken,
    ) -> InfraResult<HostInspection> {
        Ok(HostInspection {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            identity: HostIdentity {
                hostname: "dookie".into(),
                operating_system: "Linux".into(),
                kernel_release: "6.0".into(),
                architecture: "x86_64".into(),
            },
            uptime_seconds: 123.0,
            memory: HostMemory {
                total_bytes: 100,
                available_bytes: 40,
                used_bytes: 60,
                usage_percent: 60,
            },
            load: HostLoadAverage {
                one: 0.1,
                five: 0.2,
                fifteen: 0.3,
            },
        })
    }
}

#[async_trait]
impl HostSystemInspector for Fixture {
    async fn services(
        &self,
        _host: &HostRecord,
        _request: &ServiceListRequest,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Vec<ServiceStatus>> {
        Ok(vec![ServiceStatus {
            unit: "sshd.service".into(),
            load: "loaded".into(),
            active: "active".into(),
            sub: "running".into(),
            description: "OpenSSH".into(),
        }])
    }
    async fn network(
        &self,
        _host: &HostRecord,
        _deadline: Timestamp,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Vec<NetworkInterface>> {
        Ok(vec![NetworkInterface {
            index: 2,
            name: "eth0".into(),
            state: Some("UP".into()),
            mtu: Some(1500),
            addresses: vec![NetworkAddress {
                family: "inet".into(),
                address: "10.0.0.2".into(),
                prefix_len: 24,
            }],
        }])
    }
    async fn mounts(
        &self,
        _host: &HostRecord,
        _deadline: Timestamp,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Vec<MountInfo>> {
        Ok(vec![MountInfo {
            target: "/".into(),
            source: Some("/dev/root".into()),
            filesystem: Some("ext4".into()),
            options: Some("rw".into()),
            size_bytes: Some(1000),
            used_bytes: Some(500),
            available_bytes: Some(500),
        }])
    }
    async fn ports(
        &self,
        _host: &HostRecord,
        _request: &PortListRequest,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Vec<PortInfo>> {
        Ok(vec![PortInfo {
            protocol: "tcp".into(),
            state: "LISTEN".into(),
            local_address: "0.0.0.0:22".into(),
            peer_address: "0.0.0.0:*".into(),
            process: Some("sshd".into()),
        }])
    }
    async fn filesystem_usage(
        &self,
        _host: &HostRecord,
        _path: Option<&str>,
        _deadline: Timestamp,
        _cancellation: &CancellationToken,
    ) -> InfraResult<FilesystemUsage> {
        Ok(FilesystemUsage {
            source: "/dev/root".into(),
            filesystem: "ext4".into(),
            size_bytes: 1000,
            used_bytes: 500,
            available_bytes: 500,
            usage_percent: 50,
            target: "/".into(),
        })
    }
    async fn doctor(
        &self,
        host: &HostRecord,
        _deadline: Timestamp,
        _cancellation: &CancellationToken,
    ) -> InfraResult<DoctorReport> {
        Ok(DoctorReport {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            overall: "ok".into(),
            checks: vec![DoctorCheck {
                name: "network".into(),
                ok: true,
                summary: "network available".into(),
            }],
        })
    }
}

#[async_trait]
impl DockerClientProvider for Fixture {
    async fn client(
        &self,
        _host: &HostRecord,
        _cancellation: &CancellationToken,
    ) -> InfraResult<Arc<dyn DockerReadClient>> {
        Ok(Arc::new(MockDocker))
    }
}
