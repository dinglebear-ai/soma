use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use soma_fleet::{
    CommandExecutor, CommandOutput, CommandRequest, FleetResult, HostEndpoint, HostId,
};

use super::*;

struct MockExecutor {
    outputs: Mutex<VecDeque<CommandOutput>>,
    calls: Mutex<Vec<(String, Vec<String>)>>,
}

impl MockExecutor {
    fn new(outputs: impl IntoIterator<Item = CommandOutput>) -> Self {
        Self {
            outputs: Mutex::new(outputs.into_iter().collect()),
            calls: Mutex::new(Vec::new()),
        }
    }
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
        Ok(self.outputs.lock().unwrap().pop_front().unwrap())
    }
}

fn host() -> HostRecord {
    HostRecord::new(HostId::new("devhost").unwrap(), HostEndpoint::Local)
}

fn ok(text: &str) -> CommandOutput {
    CommandOutput::new(text.as_bytes().to_vec(), Vec::new(), Some(0), false)
}

#[tokio::test(flavor = "current_thread")]
async fn pool_dataset_and_snapshot_commands_are_discrete() {
    let executor = Arc::new(MockExecutor::new([
        ok("NAME SIZE
tank 100G
"),
        ok("NAME USED
tank/apps 1G
"),
        ok("NAME USED
tank/apps@daily 1M
"),
    ]));
    let inspector = CommandZfsInspector::new(Arc::clone(&executor));
    let deadline = soma_ops::Timestamp::from_unix_millis(100);

    let pools = inspector
        .pools(
            &host(),
            &ZfsPoolRequest::new(deadline).with_pool("tank").unwrap(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(pools.rows[0]["NAME"], "tank");

    inspector
        .datasets(
            &host(),
            &ZfsDatasetRequest::new(deadline)
                .with_pool("tank")
                .unwrap()
                .with_type(crate::ZfsDatasetType::Filesystem),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    inspector
        .snapshots(
            &host(),
            &ZfsSnapshotRequest::new(deadline)
                .with_dataset("tank/apps")
                .unwrap(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(
        executor.calls.lock().unwrap().as_slice(),
        [
            ("zpool".into(), vec!["list".into(), "tank".into()]),
            (
                "zfs".into(),
                vec![
                    "list".into(),
                    "-t".into(),
                    "filesystem".into(),
                    "-r".into(),
                    "tank".into(),
                ],
            ),
            (
                "zfs".into(),
                vec![
                    "list".into(),
                    "-t".into(),
                    "snapshot".into(),
                    "-r".into(),
                    "tank/apps".into(),
                ],
            ),
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn nonzero_truncated_and_non_utf8_outputs_fail_closed() {
    for output in [
        CommandOutput::new(Vec::new(), b"zfs not found".to_vec(), Some(1), false),
        CommandOutput::new(Vec::new(), Vec::new(), Some(0), true),
        CommandOutput::new(vec![0xff], Vec::new(), Some(0), false),
    ] {
        let inspector = CommandZfsInspector::new(Arc::new(MockExecutor::new([output])));
        assert!(
            inspector
                .pools(
                    &host(),
                    &ZfsPoolRequest::new(soma_ops::Timestamp::from_unix_millis(100)),
                    &CancellationToken::new(),
                )
                .await
                .is_err()
        );
    }
}
