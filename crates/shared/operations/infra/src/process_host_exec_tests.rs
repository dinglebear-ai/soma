use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use soma_fleet::{
    CommandExecutor, CommandOutput, CommandRequest, FleetError, FleetResult, HostEndpoint, HostId,
};
use soma_ops::{MutationSendState, OperationId, OperationName, Timestamp};

use super::*;
use crate::HostExecCommand;

struct MockExecutor {
    result: Mutex<Option<FleetResult<CommandOutput>>>,
    request: Mutex<Option<CommandRequest>>,
}

#[async_trait]
impl CommandExecutor for MockExecutor {
    async fn execute(
        &self,
        _host: &HostRecord,
        request: &CommandRequest,
        _cancellation: &CancellationToken,
    ) -> FleetResult<CommandOutput> {
        *self.request.lock().unwrap() = Some(request.clone());
        self.result.lock().unwrap().take().unwrap()
    }
}

fn host() -> HostRecord {
    HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local)
}

fn request(command: HostExecCommand, args: Vec<String>) -> HostExecRequest {
    HostExecRequest::new(
        OperationId::new(),
        OperationName::new("host.exec").unwrap(),
        command,
        args,
        None,
        Timestamp::from_unix_millis(Timestamp::now().unix_millis() + 10_000),
    )
    .unwrap()
}

#[tokio::test]
async fn typed_launcher_uses_discrete_python_arguments_and_retains_output() {
    let executor = Arc::new(MockExecutor {
        result: Mutex::new(Some(Ok(CommandOutput::new(
            b"ok".to_vec(),
            Vec::new(),
            Some(0),
            false,
        )))),
        request: Mutex::new(None),
    });
    let driver = CommandHostExec::new(executor.clone())
        .with_policy(host().id().clone(), HostExecPolicy::new(["/srv"]).unwrap());
    let receipt = driver
        .exec_host(
            &host(),
            &request(HostExecCommand::Cat, vec!["/srv/file".into()]),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(receipt.stdout, "ok");
    assert_eq!(receipt.send_state, MutationSendState::Sent);
    let captured = executor.request.lock().unwrap();
    let captured = captured.as_ref().unwrap();
    assert_eq!(captured.program(), "python3");
    assert_eq!(captured.args()[0], "-c");
    assert_eq!(captured.args()[3], "cat");
    assert_eq!(
        captured.args().last().map(String::as_str),
        Some("/srv/file")
    );
}

#[tokio::test]
async fn policy_and_executor_failures_preserve_send_truth() {
    let executor = Arc::new(MockExecutor {
        result: Mutex::new(Some(Err(FleetError::Cancelled))),
        request: Mutex::new(None),
    });
    let driver = CommandHostExec::new(executor.clone());
    let failure = driver
        .exec_host(
            &host(),
            &request(HostExecCommand::Hostname, Vec::new()),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(failure.send_state(), MutationSendState::NotSent);

    let driver = driver.with_policy(host().id().clone(), HostExecPolicy::new(["/srv"]).unwrap());
    let failure = driver
        .exec_host(
            &host(),
            &request(HostExecCommand::Hostname, Vec::new()),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert_eq!(failure.send_state(), MutationSendState::Unknown);
}

#[tokio::test]
async fn local_launcher_reads_bound_files_and_refuses_symlink_escape() {
    use std::fs;
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let allowed = root.path().join("allowed.txt");
    let escaped = root.path().join("escape.txt");
    let secret = outside.path().join("secret.txt");
    fs::write(&allowed, b"bound").unwrap();
    fs::write(&secret, b"secret").unwrap();
    symlink(&secret, &escaped).unwrap();

    let driver = CommandHostExec::new(Arc::new(soma_fleet::LocalProcessDriver)).with_policy(
        host().id().clone(),
        HostExecPolicy::new([root.path()]).unwrap(),
    );
    let receipt = driver
        .exec_host(
            &host(),
            &request(
                HostExecCommand::Cat,
                vec![allowed.to_string_lossy().into_owned()],
            ),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(receipt.exit_code, Some(0));
    assert_eq!(receipt.stdout, "bound");

    let receipt = driver
        .exec_host(
            &host(),
            &request(
                HostExecCommand::Cat,
                vec![escaped.to_string_lossy().into_owned()],
            ),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_ne!(receipt.exit_code, Some(0));
    assert!(!receipt.stderr.contains("secret"));
}
