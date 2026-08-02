use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use soma_fleet::{CommandOutput, CommandRequest, FleetResult, HostEndpoint, HostId};

use super::*;
use crate::ComposeProjectRef;

struct MockExecutor {
    outputs: Mutex<VecDeque<FleetResult<CommandOutput>>>,
    calls: Mutex<Vec<(String, Vec<String>)>>,
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
            .unwrap()
            .push((request.program().to_owned(), request.args().to_vec()));
        self.outputs.lock().unwrap().pop_front().unwrap()
    }
}

fn host() -> HostRecord {
    HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local)
}

fn request(action: ComposeMutationAction) -> ComposeMutationRequest {
    ComposeMutationRequest::new(
        ComposeProjectRef::new("soma", "/srv/soma/compose.yaml").unwrap(),
        action,
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
    )
}

fn output(exit: i32) -> CommandOutput {
    CommandOutput::new(Vec::new(), b"failure".to_vec(), Some(exit), false)
}

#[tokio::test(flavor = "current_thread")]
async fn up_and_restart_use_discrete_compose_arguments() {
    let executor = Arc::new(MockExecutor {
        outputs: Mutex::new(VecDeque::from([Ok(output(0)), Ok(output(0))])),
        calls: Mutex::new(Vec::new()),
    });
    let client = CommandComposeInspector::new(Arc::clone(&executor));
    for action in [ComposeMutationAction::Up, ComposeMutationAction::Restart] {
        let receipt = client
            .mutate_compose(&host(), &request(action), &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(receipt.send_state, MutationSendState::Sent);
    }
    let calls = executor.calls.lock().unwrap();
    assert_eq!(
        calls[0],
        (
            "docker".into(),
            vec![
                "compose".into(),
                "-f".into(),
                "/srv/soma/compose.yaml".into(),
                "up".into(),
                "-d".into(),
            ]
        )
    );
    assert_eq!(calls[1].1.last().map(String::as_str), Some("restart"));
}

#[tokio::test(flavor = "current_thread")]
async fn nonzero_compose_exit_is_known_sent_failure() {
    let executor = Arc::new(MockExecutor {
        outputs: Mutex::new(VecDeque::from([Ok(output(1))])),
        calls: Mutex::new(Vec::new()),
    });
    let failure = CommandComposeInspector::new(executor)
        .mutate_compose(
            &host(),
            &request(ComposeMutationAction::Restart),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(failure.send_state(), MutationSendState::Sent);
}

#[tokio::test(flavor = "current_thread")]
async fn cancelled_compose_request_never_reaches_executor() {
    let executor = Arc::new(MockExecutor {
        outputs: Mutex::new(VecDeque::new()),
        calls: Mutex::new(Vec::new()),
    });
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let failure = CommandComposeInspector::new(Arc::clone(&executor))
        .mutate_compose(&host(), &request(ComposeMutationAction::Up), &cancellation)
        .await
        .unwrap_err();
    assert_eq!(failure.send_state(), MutationSendState::NotSent);
    assert!(executor.calls.lock().unwrap().is_empty());
}
