use std::collections::BTreeMap;

use async_trait::async_trait;
use soma_fleet::HostRecord;
use soma_infra::*;
use tokio_util::sync::CancellationToken;

pub(crate) struct MockDocker;

fn docker_info(host: &HostRecord) -> DockerSystemInfo {
    DockerSystemInfo {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        daemon_id: Some("daemon".into()),
        name: Some("dookie".into()),
        server_version: Some("1".into()),
        operating_system: Some("Linux".into()),
        architecture: Some("x86_64".into()),
        kernel_version: Some("6".into()),
        storage_driver: Some("overlay2".into()),
        containers: 1,
        containers_running: 1,
        containers_paused: 0,
        containers_stopped: 0,
        images: 1,
        cpus: 8,
        memory_total_bytes: 1024,
    }
}

#[async_trait]
impl DockerSystemReader for MockDocker {
    async fn system_info(
        &self,
        host: &HostRecord,
        _: &CancellationToken,
    ) -> InfraResult<DockerSystemInfo> {
        Ok(docker_info(host))
    }
}
#[async_trait]
impl ContainerReader for MockDocker {
    async fn list_containers(
        &self,
        host: &HostRecord,
        _: &ContainerListOptions,
        _: &CancellationToken,
    ) -> InfraResult<Vec<ContainerSummary>> {
        Ok(vec![ContainerSummary {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            id: Some("abc".into()),
            names: vec!["/soma".into()],
            image: Some("soma:latest".into()),
            image_id: Some("sha256:1".into()),
            command: Some("serve".into()),
            created_unix_seconds: Some(1),
            state: ContainerState::Running,
            status: Some("Up".into()),
            labels: BTreeMap::new(),
        }])
    }
    async fn inspect_container(
        &self,
        host: &HostRecord,
        _: &str,
        _: &CancellationToken,
    ) -> InfraResult<ContainerInspect> {
        Ok(ContainerInspect {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            id: Some("abc".into()),
            name: Some("/soma".into()),
            created: Some("now".into()),
            path: Some("/app".into()),
            args: vec!["serve".into()],
            image: Some("sha256:1".into()),
            state: ContainerState::Running,
            pid: Some(1),
            exit_code: Some(0),
            restart_count: Some(0),
            labels: BTreeMap::new(),
        })
    }
    async fn top_container(
        &self,
        host: &HostRecord,
        container: &str,
        _: &CancellationToken,
    ) -> InfraResult<ContainerProcessTable> {
        Ok(ContainerProcessTable {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            container: container.into(),
            titles: vec!["PID".into(), "CMD".into()],
            processes: vec![vec!["1".into(), "serve".into()]],
        })
    }
}
#[async_trait]
impl ImageReader for MockDocker {
    async fn list_images(
        &self,
        host: &HostRecord,
        _: &ImageListOptions,
        _: &CancellationToken,
    ) -> InfraResult<Vec<ImageSummary>> {
        Ok(vec![ImageSummary {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            id: "sha256:1".into(),
            repo_tags: vec!["soma:latest".into()],
            repo_digests: vec![],
            created_unix_seconds: 1,
            size_bytes: 100,
            containers: 1,
            labels: BTreeMap::new(),
        }])
    }
}
#[async_trait]
impl NetworkReader for MockDocker {
    async fn list_networks(
        &self,
        host: &HostRecord,
        _: &CancellationToken,
    ) -> InfraResult<Vec<NetworkSummary>> {
        Ok(vec![NetworkSummary {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            id: Some("n1".into()),
            name: Some("bridge".into()),
            driver: Some("bridge".into()),
            scope: Some("local".into()),
            internal: Some(false),
            attachable: Some(true),
            labels: BTreeMap::new(),
        }])
    }
}
#[async_trait]
impl VolumeReader for MockDocker {
    async fn list_volumes(
        &self,
        host: &HostRecord,
        _: &CancellationToken,
    ) -> InfraResult<Vec<VolumeSummary>> {
        Ok(vec![VolumeSummary {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            name: "data".into(),
            driver: "local".into(),
            mountpoint: "/data".into(),
            scope: Some("local".into()),
            labels: BTreeMap::new(),
        }])
    }
}
#[async_trait]
impl DockerTelemetryReader for MockDocker {
    async fn disk_usage(
        &self,
        host: &HostRecord,
        _: &CancellationToken,
    ) -> InfraResult<DockerDiskUsage> {
        Ok(DockerDiskUsage {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            layers_size_bytes: 1,
            images: DockerUsageCategory {
                count: 1,
                size_bytes: 100,
            },
            containers: DockerUsageCategory {
                count: 1,
                size_bytes: 10,
            },
            volumes: DockerUsageCategory::default(),
            build_cache: DockerUsageCategory::default(),
        })
    }
    async fn container_logs(
        &self,
        host: &HostRecord,
        container: &str,
        _: &ContainerLogOptions,
        _: &CancellationToken,
    ) -> InfraResult<ContainerLogs> {
        Ok(ContainerLogs {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            container: container.into(),
            lines: vec!["ready".into()],
            truncated: false,
        })
    }
    async fn container_stats(
        &self,
        host: &HostRecord,
        container: &str,
        _: &CancellationToken,
    ) -> InfraResult<ContainerStatsSnapshot> {
        Ok(ContainerStatsSnapshot {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            container: container.into(),
            read_at: Some("2026-08-02T00:00:00Z".into()),
            pids_current: 1,
            memory_usage_bytes: 10,
            memory_limit_bytes: 100,
            cpu_total_usage: 5,
            system_cpu_usage: 50,
            online_cpus: 8,
            network_rx_bytes: 1,
            network_tx_bytes: 2,
            block_read_bytes: 3,
            block_write_bytes: 4,
        })
    }
}
