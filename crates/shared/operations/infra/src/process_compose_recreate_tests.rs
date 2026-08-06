use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use soma_fleet::{CommandOutput, CommandRequest, FleetResult, HostEndpoint, HostId};
use soma_ops::{OperationId, OperationName};

use super::*;
use crate::{ComposeProjectRef, ComposeRecreateFingerprint};

struct MockExecutor {
    outputs: Mutex<VecDeque<FleetResult<CommandOutput>>>,
    calls: Mutex<Vec<(String, Vec<String>)>>,
}

#[async_trait]
impl CommandExecutor for MockExecutor {
    async fn execute(
        &self,
        _: &HostRecord,
        request: &CommandRequest,
        _: &CancellationToken,
    ) -> FleetResult<CommandOutput> {
        self.calls
            .lock()
            .unwrap()
            .push((request.program().to_owned(), request.args().to_vec()));
        self.outputs.lock().unwrap().pop_front().unwrap()
    }
}

fn host() -> HostRecord {
    HostRecord::new(HostId::new("devhost").unwrap(), HostEndpoint::Local)
}

fn request() -> ComposeRecreateRequest {
    ComposeRecreateRequest::new(
        OperationId::new(),
        OperationName::new("compose.recreate").unwrap(),
        ComposeProjectRef::new("soma", "/srv/soma/compose.yaml").unwrap(),
        ComposeRecreateFingerprint::new("soma", vec!["api".into()], "a".repeat(64)).unwrap(),
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
    )
}

#[tokio::test]
async fn force_recreate_uses_discrete_arguments() {
    let executor = Arc::new(MockExecutor {
        outputs: Mutex::new(VecDeque::from([Ok(CommandOutput::new(
            Vec::new(),
            Vec::new(),
            Some(0),
            false,
        ))])),
        calls: Mutex::new(Vec::new()),
    });
    let receipt = CommandComposeInspector::new(Arc::clone(&executor))
        .recreate_compose(&host(), &request(), &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(receipt.send_state, MutationSendState::Sent);
    assert_eq!(
        executor.calls.lock().unwrap()[0].1,
        vec![
            "compose",
            "-f",
            "/srv/soma/compose.yaml",
            "up",
            "-d",
            "--force-recreate"
        ]
    );
}
