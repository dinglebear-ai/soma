use std::collections::BTreeMap;
use std::path::Path;

use async_trait::async_trait;
use soma_fleet::HostRecord;
use soma_infra::*;
use soma_ops::Timestamp;
use tokio_util::sync::CancellationToken;

use crate::runtime_test_support::Fixture;

#[async_trait]
impl ComposeInspector for Fixture {
    async fn list_projects(
        &self,
        host: &HostRecord,
        _: Timestamp,
        _: &CancellationToken,
    ) -> InfraResult<Vec<ComposeProject>> {
        Ok(vec![ComposeProject {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            name: "soma".into(),
            status: Some("running".into()),
            config_files: vec!["/srv/soma/compose.yaml".into()],
        }])
    }
    async fn status(
        &self,
        host: &HostRecord,
        project: &ComposeProjectRef,
        _: Option<&str>,
        _: Timestamp,
        _: &CancellationToken,
    ) -> InfraResult<ComposeStatus> {
        Ok(ComposeStatus {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: project.name().into(),
            services: vec![ComposeServiceStatus {
                service: "api".into(),
                container_name: Some("soma-api-1".into()),
                state: Some("running".into()),
                health: Some("healthy".into()),
                exit_code: Some(0),
                image: Some("soma:latest".into()),
            }],
        })
    }
    async fn config(
        &self,
        host: &HostRecord,
        project: &ComposeProjectRef,
        _: Timestamp,
        _: &CancellationToken,
    ) -> InfraResult<ComposeConfig> {
        Ok(ComposeConfig {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: project.name().into(),
            services: BTreeMap::new(),
            networks: vec!["default".into()],
            volumes: vec!["data".into()],
        })
    }
    async fn logs(
        &self,
        host: &HostRecord,
        project: &ComposeProjectRef,
        _: &ComposeLogRequest,
        _: &CancellationToken,
    ) -> InfraResult<ComposeLogs> {
        Ok(ComposeLogs {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            project: project.name().into(),
            lines: vec!["api | ready".into()],
            truncated: false,
        })
    }
}

#[async_trait]
impl FilesystemQueryInspector for Fixture {
    async fn read_path(
        &self,
        host: &HostRecord,
        path: &Path,
        request: &PathReadRequest,
        _: &CancellationToken,
    ) -> InfraResult<PathRead> {
        let tree = request.tree();
        Ok(PathRead {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            path: path.to_path_buf(),
            kind: if tree {
                FileKind::Directory
            } else {
                FileKind::File
            },
            content: if tree {
                Vec::new()
            } else {
                b"hello\n".to_vec()
            },
            entries: if tree {
                vec![format!("{}/child", path.display())]
            } else {
                Vec::new()
            },
            size_bytes: 6,
            truncated: false,
        })
    }
    async fn find(
        &self,
        host: &HostRecord,
        path: &Path,
        _: &FileFindRequest,
        _: &CancellationToken,
    ) -> InfraResult<FileSearch> {
        Ok(FileSearch {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            path: path.to_path_buf(),
            items: vec![path.join("a.log")],
            truncated: false,
        })
    }
    async fn tail(
        &self,
        host: &HostRecord,
        path: &Path,
        _: &FileTailRequest,
        _: &CancellationToken,
    ) -> InfraResult<FileTail> {
        Ok(FileTail {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            path: path.to_path_buf(),
            content: b"tail\n".to_vec(),
            line_count: 1,
            truncated: false,
        })
    }
}

#[async_trait]
impl ProcessInspector for Fixture {
    async fn list_processes(
        &self,
        host: &HostRecord,
        request: &ProcessListRequest,
        _: &CancellationToken,
    ) -> InfraResult<ProcessSnapshot> {
        Ok(ProcessSnapshot {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            sort: request.sort(),
            rows: vec![ProcessRow {
                user: "devuser".into(),
                pid: 1,
                cpu_percent: 1.0,
                memory_percent: 2.0,
                virtual_size_kib: 10,
                resident_size_kib: 5,
                tty: "?".into(),
                state: "S".into(),
                start: "00:00".into(),
                cpu_time: "0:01".into(),
                command: "soma".into(),
            }],
            truncated: false,
        })
    }
}

#[async_trait]
impl LogReader for Fixture {
    async fn read_logs(
        &self,
        host: &HostRecord,
        request: &LogReadRequest,
        _: &CancellationToken,
    ) -> InfraResult<LogRead> {
        Ok(LogRead {
            host: host.id().clone(),
            topology_revision: host.revision().clone(),
            source: request.source(),
            source_path: None,
            lines: vec!["ready".into()],
            truncated: false,
            permission: None,
        })
    }
}

fn zfs(host: &HostRecord) -> ZfsTable {
    ZfsTable {
        host: host.id().clone(),
        topology_revision: host.revision().clone(),
        columns: vec!["NAME".into()],
        rows: vec![BTreeMap::from([("NAME".into(), "tank".into())])],
        truncated: false,
    }
}

#[async_trait]
impl ZfsInspector for Fixture {
    async fn pools(
        &self,
        host: &HostRecord,
        _: &ZfsPoolRequest,
        _: &CancellationToken,
    ) -> InfraResult<ZfsTable> {
        Ok(zfs(host))
    }
    async fn datasets(
        &self,
        host: &HostRecord,
        _: &ZfsDatasetRequest,
        _: &CancellationToken,
    ) -> InfraResult<ZfsTable> {
        Ok(zfs(host))
    }
    async fn snapshots(
        &self,
        host: &HostRecord,
        _: &ZfsSnapshotRequest,
        _: &CancellationToken,
    ) -> InfraResult<ZfsTable> {
        Ok(zfs(host))
    }
}
