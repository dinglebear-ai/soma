use super::*;
use crate::{BuildContextFingerprint, ComposeBuildArtifact, ComposeProjectRef};
use async_trait::async_trait;
use soma_fleet::{CommandOutput, FleetResult, HostEndpoint, HostId};
use soma_ops::{NoopProgressSink, OperationId, OperationName};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

struct Mock {
    outputs: Mutex<VecDeque<FleetResult<CommandOutput>>>,
    args: Mutex<Vec<Vec<String>>>,
}
#[async_trait]
impl CommandExecutor for Mock {
    async fn execute(
        &self,
        _: &HostRecord,
        request: &CommandRequest,
        _: &CancellationToken,
    ) -> FleetResult<CommandOutput> {
        self.args.lock().unwrap().push(request.args().to_vec());
        self.outputs.lock().unwrap().pop_front().unwrap()
    }
}
fn host() -> HostRecord {
    HostRecord::new(HostId::new("devhost").unwrap(), HostEndpoint::Local)
}
fn request() -> ComposeBuildRequest {
    let fp = BuildContextFingerprint {
        host: host().id().clone(),
        topology_revision: host().revision().clone(),
        path: "/srv/app".into(),
        sha256: "a".repeat(64),
        file_count: 1,
        byte_count: 1,
    };
    ComposeBuildRequest::new(
        OperationId::new(),
        OperationName::new("compose.build").unwrap(),
        ComposeProjectRef::new("soma", "/srv/soma/compose.yaml").unwrap(),
        Some("api".into()),
        vec![ComposeBuildArtifact {
            service: "api".into(),
            image: "soma-api:v1".into(),
            context: "/srv/app".into(),
            fingerprint: fp,
        }],
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
    )
    .unwrap()
}
#[tokio::test]
async fn compose_build_uses_discrete_service_separator() {
    let mock = Arc::new(Mock {
        outputs: Mutex::new(VecDeque::from([Ok(CommandOutput::new(
            Vec::new(),
            Vec::new(),
            Some(0),
            false,
        ))])),
        args: Mutex::new(Vec::new()),
    });
    CommandComposeBuildMutator::new(mock.clone())
        .build_compose(
            &host(),
            &request(),
            &NoopProgressSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(
        mock.args.lock().unwrap()[0],
        vec![
            "compose",
            "--progress",
            "plain",
            "-f",
            "/srv/soma/compose.yaml",
            "build",
            "--",
            "api"
        ]
    );
}
