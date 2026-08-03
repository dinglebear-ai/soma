use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use soma_fleet::{
    CommandExecutor, CommandOutput, CommandRequest, FleetResult, HostEndpoint, HostId,
};

use super::*;

struct MockExecutor {
    output: CommandOutput,
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
        Ok(self.output.clone())
    }
}

fn host() -> HostRecord {
    HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local)
}

#[tokio::test(flavor = "current_thread")]
async fn driver_uses_allowlisted_sort_and_parses_rows() {
    let executor = Arc::new(MockExecutor {
        output: CommandOutput::new(
            concat!(
                "USER PID %CPU %MEM VSZ RSS TTY STAT START TIME COMMAND
",
                "jmagar 22 10.5 3.5 2000 1000 pts/0 Sl 10:01 1:02 soma serve
",
            )
            .as_bytes()
            .to_vec(),
            Vec::new(),
            Some(0),
            false,
        ),
        calls: Mutex::new(Vec::new()),
    });
    let inspector = CommandProcessInspector::new(Arc::clone(&executor));
    let request = ProcessListRequest::new(soma_ops::Timestamp::from_unix_millis(100))
        .with_sort(crate::ProcessSort::Memory);
    let snapshot = inspector
        .list_processes(&host(), &request, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(snapshot.rows[0].pid, 22);
    assert_eq!(
        executor.calls.lock().unwrap()[0],
        (
            "ps".into(),
            vec!["aux".into(), "--sort".into(), "-mem".into()]
        )
    );
}

#[tokio::test(flavor = "current_thread")]
async fn driver_rejects_nonzero_truncated_and_non_utf8_output() {
    for output in [
        CommandOutput::new(Vec::new(), b"denied".to_vec(), Some(1), false),
        CommandOutput::new(Vec::new(), Vec::new(), Some(0), true),
        CommandOutput::new(vec![0xff], Vec::new(), Some(0), false),
    ] {
        let inspector = CommandProcessInspector::new(Arc::new(MockExecutor {
            output,
            calls: Mutex::new(Vec::new()),
        }));
        assert!(
            inspector
                .list_processes(
                    &host(),
                    &ProcessListRequest::new(soma_ops::Timestamp::from_unix_millis(100)),
                    &CancellationToken::new(),
                )
                .await
                .is_err()
        );
    }
}
