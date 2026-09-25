use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use soma_fleet::{
    CommandExecutor, CommandOutput, CommandRequest, FleetResult, HostEndpoint, HostId, HostRecord,
};
use soma_ops::{OperationId, OperationName, Timestamp};

use super::*;
use crate::{ComposeProjectRef, ComposeRecreateFingerprint};

#[derive(Default)]
struct MockExecutor(Mutex<Vec<CommandRequest>>);

#[async_trait]
impl CommandExecutor for MockExecutor {
    async fn execute(
        &self,
        _host: &HostRecord,
        request: &CommandRequest,
        _cancellation: &CancellationToken,
    ) -> FleetResult<CommandOutput> {
        self.0.lock().unwrap().push(request.clone());
        Ok(CommandOutput::new(Vec::new(), Vec::new(), Some(0), false))
    }
}

#[tokio::test]
async fn compose_down_uses_discrete_volume_argument() {
    let executor = Arc::new(MockExecutor::default());
    let inspector = CommandComposeInspector::new(executor.clone());
    let host = HostRecord::new(HostId::new("devhost").unwrap(), HostEndpoint::Local);
    let request = ComposeDownRequest::new(
        OperationId::new(),
        OperationName::new("compose.down").unwrap(),
        ComposeProjectRef::new("soma", "/srv/soma/compose.yaml").unwrap(),
        ComposeRecreateFingerprint::new("soma", vec!["api".into()], "a".repeat(64)).unwrap(),
        true,
        true,
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
    )
    .unwrap();
    inspector
        .down_compose(&host, &request, &CancellationToken::new())
        .await
        .unwrap();
    let calls = executor.0.lock().unwrap();
    assert_eq!(
        calls[0].args(),
        [
            "compose",
            "-f",
            "/srv/soma/compose.yaml",
            "down",
            "--volumes"
        ]
    );
}
