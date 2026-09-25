use super::*;
use crate::BuildContextFingerprint;
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
fn request() -> ImageBuildRequest {
    let context = BuildContextFingerprint {
        host: host().id().clone(),
        topology_revision: host().revision().clone(),
        path: "/srv/app".into(),
        sha256: "a".repeat(64),
        file_count: 1,
        byte_count: 1,
    };
    ImageBuildRequest::new(
        OperationId::new(),
        OperationName::new("docker.build").unwrap(),
        "/srv/app".into(),
        Some("Dockerfile".into()),
        "app:v1",
        true,
        context,
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
    )
    .unwrap()
}
#[tokio::test]
async fn docker_build_uses_discrete_bounded_arguments() {
    let mock = Arc::new(Mock {
        outputs: Mutex::new(VecDeque::from([Ok(CommandOutput::new(
            b"ok".to_vec(),
            Vec::new(),
            Some(0),
            false,
        ))])),
        args: Mutex::new(Vec::new()),
    });
    let receipt = CommandImageBuildMutator::new(mock.clone())
        .build_image(
            &host(),
            &request(),
            &NoopProgressSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(receipt.send_state, MutationSendState::Sent);
    assert_eq!(
        mock.args.lock().unwrap()[0],
        vec![
            "build",
            "--progress=plain",
            "-t",
            "app:v1",
            "--no-cache",
            "-f",
            "/srv/app/Dockerfile",
            "/srv/app"
        ]
    );
}
#[tokio::test]
async fn nonzero_build_exit_is_known_sent_failure() {
    let mock = Arc::new(Mock {
        outputs: Mutex::new(VecDeque::from([Ok(CommandOutput::new(
            Vec::new(),
            b"failed".to_vec(),
            Some(1),
            false,
        ))])),
        args: Mutex::new(Vec::new()),
    });
    let error = CommandImageBuildMutator::new(mock)
        .build_image(
            &host(),
            &request(),
            &NoopProgressSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.send_state(), MutationSendState::Sent);
}
