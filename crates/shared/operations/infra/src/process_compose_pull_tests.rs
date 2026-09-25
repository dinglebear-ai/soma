use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use soma_fleet::{CommandOutput, CommandRequest, FleetResult, HostEndpoint, HostId};
use soma_ops::{NoopProgressSink, OperationId, OperationName};

use super::*;
use crate::ComposeProjectRef;

struct MockExecutor {
    outputs: Mutex<VecDeque<FleetResult<CommandOutput>>>,
    calls: Mutex<Vec<Vec<String>>>,
}

#[async_trait]
impl CommandExecutor for MockExecutor {
    async fn execute(
        &self,
        _host: &HostRecord,
        request: &CommandRequest,
        _cancellation: &CancellationToken,
    ) -> FleetResult<CommandOutput> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(request.args().to_vec());
        self.outputs
            .lock()
            .expect("outputs lock")
            .pop_front()
            .expect("output fixture")
    }
}

fn host() -> HostRecord {
    HostRecord::new(HostId::new("devhost").unwrap(), HostEndpoint::Local)
}

fn request(service: Option<&str>) -> ComposePullRequest {
    ComposePullRequest::new(
        OperationId::new(),
        OperationName::new("compose.pull").unwrap(),
        ComposeProjectRef::new("soma", "/srv/soma/compose.yaml").unwrap(),
        service.map(str::to_owned),
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
    )
    .unwrap()
}

#[tokio::test]
async fn compose_pull_uses_discrete_arguments_and_service_separator() {
    let executor = Arc::new(MockExecutor {
        outputs: Mutex::new(VecDeque::from([Ok(CommandOutput::new(
            Vec::new(),
            Vec::new(),
            Some(0),
            false,
        ))])),
        calls: Mutex::new(Vec::new()),
    });
    let client = CommandComposeInspector::new(Arc::clone(&executor));
    let receipt = client
        .pull_compose_images(
            &host(),
            &request(Some("api")),
            &NoopProgressSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(receipt.send_state, MutationSendState::Sent);
    assert_eq!(
        executor.calls.lock().unwrap()[0],
        vec![
            "compose",
            "-f",
            "/srv/soma/compose.yaml",
            "pull",
            "--",
            "api"
        ]
    );
}

#[tokio::test]
async fn nonzero_pull_exit_is_known_sent_failure() {
    let executor = Arc::new(MockExecutor {
        outputs: Mutex::new(VecDeque::from([Ok(CommandOutput::new(
            Vec::new(),
            b"denied".to_vec(),
            Some(1),
            false,
        ))])),
        calls: Mutex::new(Vec::new()),
    });
    let client = CommandComposeInspector::new(executor);
    let error = client
        .pull_compose_images(
            &host(),
            &request(None),
            &NoopProgressSink,
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.send_state(), MutationSendState::Sent);
}
