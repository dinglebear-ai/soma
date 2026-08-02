use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use soma_fleet::{
    CommandExecutor, CommandOutput, CommandRequest, FleetResult, HostEndpoint, HostId,
};

use super::*;
use crate::{JournalFilters, JournalPriority};

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
    HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local)
}

fn output(stdout: &str, stderr: &str, code: i32) -> CommandOutput {
    CommandOutput::new(
        stdout.as_bytes().to_vec(),
        stderr.as_bytes().to_vec(),
        Some(code),
        false,
    )
}

#[tokio::test(flavor = "current_thread")]
async fn file_logs_fall_back_and_filter_locally() {
    let executor = Arc::new(MockExecutor::new([
        output("", "No such file or directory", 1),
        output(
            "one info
two error
three error
",
            "",
            0,
        ),
    ]));
    let reader = CommandLogReader::new(Arc::clone(&executor));
    let request = LogReadRequest::new(
        LogSource::Syslog,
        soma_ops::Timestamp::from_unix_millis(100),
    )
    .with_lines(1)
    .unwrap()
    .with_grep("error")
    .unwrap();
    let result = reader
        .read_logs(&host(), &request, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.source_path, Some("/var/log/messages".into()));
    assert_eq!(result.lines, vec!["three error"]);
    assert!(result.truncated);
    assert_eq!(executor.calls.lock().unwrap().len(), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn journal_builds_discrete_validated_arguments() {
    let executor = Arc::new(MockExecutor::new([output(
        "line
", "", 0,
    )]));
    let reader = CommandLogReader::new(Arc::clone(&executor));
    let filters = JournalFilters::default()
        .with_unit("soma.service")
        .unwrap()
        .with_priority(JournalPriority::Err)
        .with_since("-1h")
        .unwrap();
    let request = LogReadRequest::new(
        LogSource::Journal,
        soma_ops::Timestamp::from_unix_millis(100),
    )
    .with_journal_filters(filters)
    .unwrap();
    reader
        .read_logs(&host(), &request, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        executor.calls.lock().unwrap()[0],
        (
            "journalctl".into(),
            vec![
                "-n".into(),
                "100".into(),
                "--no-pager".into(),
                "-u".into(),
                "soma.service".into(),
                "-p".into(),
                "err".into(),
                "--since".into(),
                "-1h".into(),
            ]
        )
    );
}

#[tokio::test(flavor = "current_thread")]
async fn dmesg_permission_failure_is_structured() {
    let reader = CommandLogReader::new(Arc::new(MockExecutor::new([output(
        "",
        "dmesg: read kernel buffer failed: Operation not permitted",
        1,
    )])));
    let result = reader
        .read_logs(
            &host(),
            &LogReadRequest::new(LogSource::Dmesg, soma_ops::Timestamp::from_unix_millis(100)),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(result.permission.is_some());
    assert!(result.lines.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn unexpected_command_failure_remains_typed() {
    let reader = CommandLogReader::new(Arc::new(MockExecutor::new([output(
        "",
        "journal failed",
        2,
    )])));
    assert!(matches!(
        reader
            .read_logs(
                &host(),
                &LogReadRequest::new(
                    LogSource::Journal,
                    soma_ops::Timestamp::from_unix_millis(100),
                ),
                &CancellationToken::new(),
            )
            .await,
        Err(InfraError::CommandFailed { .. })
    ));
}
