use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use soma_fleet::{
    CommandExecutor, CommandOutput, CommandRequest, FleetResult, HostEndpoint, HostId, SshEndpoint,
};

use super::*;

struct MockExecutor {
    outputs: Mutex<VecDeque<CommandOutput>>,
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
        self.calls.lock().unwrap().push(request.args().to_vec());
        Ok(self.outputs.lock().unwrap().pop_front().unwrap())
    }
}

fn output(value: &str) -> CommandOutput {
    CommandOutput::new(value.as_bytes().to_vec(), Vec::new(), Some(0), false)
}
fn host() -> HostRecord {
    HostRecord::new(
        HostId::new("remote").unwrap(),
        HostEndpoint::Ssh(SshEndpoint::new("remote").unwrap()),
    )
}
fn deadline() -> soma_ops::Timestamp {
    soma_ops::Timestamp::from_unix_millis(10_000)
}

#[tokio::test(flavor = "current_thread")]
async fn remote_query_driver_reads_finds_and_tails_with_descriptor_script() {
    let executor = Arc::new(MockExecutor {
        outputs: Mutex::new(VecDeque::from([
            output(
                r#"{"kind":"file","content_b64":"aGVsbG8=","entries":[],"size":5,"truncated":false}"#,
            ),
            output(r#"{"kind":"directory","entries":["/srv/a.log"],"size":0,"truncated":false}"#),
            output(
                r#"{"kind":"file","content_b64":"dGFpbAo=","entries":[],"size":99,"truncated":true,"line_count":1}"#,
            ),
        ])),
        calls: Mutex::new(Vec::new()),
    });
    let inspector = CommandFilesystemQueryInspector::new(
        Arc::clone(&executor),
        FileReadPolicy::new(["/srv"])
            .unwrap()
            .with_preview_limit(1024)
            .unwrap(),
    );
    let cancellation = CancellationToken::new();
    let read = inspector
        .read_path(
            &host(),
            Path::new("/srv/a.txt"),
            &PathReadRequest::new(deadline()),
            &cancellation,
        )
        .await
        .unwrap();
    assert_eq!(read.content, b"hello");
    let found = inspector
        .find(
            &host(),
            Path::new("/srv"),
            &FileFindRequest::new("*.log", deadline())
                .unwrap()
                .with_depth(4)
                .unwrap(),
            &cancellation,
        )
        .await
        .unwrap();
    assert_eq!(found.items, vec![PathBuf::from("/srv/a.log")]);
    let tail = inspector
        .tail(
            &host(),
            Path::new("/srv/a.log"),
            &FileTailRequest::new(deadline()),
            &cancellation,
        )
        .await
        .unwrap();
    assert_eq!(
        tail.content,
        b"tail
"
    );
    assert!(tail.truncated);

    let calls = executor.calls.lock().unwrap();
    assert!(calls.iter().all(|args| args[0] == "-c"));
    assert_eq!(calls[1][2], "find");
    assert_eq!(calls[1].last().map(String::as_str), Some("4"));
}

#[test]
fn query_output_failures_remain_typed() {
    let host = host();
    assert!(matches!(
        parse_output(
            &host,
            CommandOutput::new(Vec::new(), b"bad".to_vec(), Some(1), false)
        ),
        Err(InfraError::CommandFailed { .. })
    ));
    assert!(matches!(
        parse_output(
            &host,
            CommandOutput::new(b"{}".to_vec(), Vec::new(), Some(0), true)
        ),
        Err(InfraError::Parse { .. })
    ));
}
