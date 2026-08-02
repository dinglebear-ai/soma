use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use soma_fleet::{
    CommandExecutor, CommandOutput, CommandRequest, FleetResult, HostEndpoint, HostId,
};

use super::*;

struct MockExecutor {
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
        let text = match request.args() {
            [a, b, c, d] if [a, b, c, d] == ["compose", "ls", "--format", "json"] => {
                r#"[{"Name":"soma","Status":"running","ConfigFiles":"/srv/soma/compose.yaml"}]"#
            }
            args if args.iter().any(|value| value == "ps") => {
                r#"[{"Service":"api","State":"running","ExitCode":0}]"#
            }
            args if args.iter().any(|value| value == "config") => {
                r#"{"services":{"api":{"image":"soma:latest"}},"networks":{},"volumes":{}}"#
            }
            _ => panic!("unexpected args: {:?}", request.args()),
        };
        Ok(CommandOutput::new(
            text.as_bytes().to_vec(),
            Vec::new(),
            Some(0),
            false,
        ))
    }
}

fn host() -> HostRecord {
    HostRecord::new(HostId::new("dookie").unwrap(), HostEndpoint::Local)
}

fn deadline() -> Timestamp {
    Timestamp::from_unix_millis(10_000)
}

#[tokio::test(flavor = "current_thread")]
async fn driver_uses_discrete_compose_arguments_and_parses_results() {
    let executor = Arc::new(MockExecutor {
        calls: Mutex::new(Vec::new()),
    });
    let inspector = CommandComposeInspector::new(Arc::clone(&executor));
    let project = ComposeProjectRef::new("soma", "/srv/soma/compose.yaml").unwrap();

    let projects = inspector
        .list_projects(&host(), deadline(), &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(projects[0].name, "soma");

    let status = inspector
        .status(
            &host(),
            &project,
            Some("api"),
            deadline(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(status.services[0].service, "api");

    let config = inspector
        .config(&host(), &project, deadline(), &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(config.services["api"].image.as_deref(), Some("soma:latest"));

    let calls = executor.calls.lock().unwrap();
    assert_eq!(
        calls[1],
        vec![
            "compose",
            "-f",
            "/srv/soma/compose.yaml",
            "--project-name",
            "soma",
            "ps",
            "--format",
            "json",
            "--",
            "api"
        ]
    );
    assert_eq!(
        calls[2],
        vec![
            "compose",
            "-f",
            "/srv/soma/compose.yaml",
            "--project-name",
            "soma",
            "config",
            "--format",
            "json"
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_service_is_rejected_before_executor_call() {
    let executor = Arc::new(MockExecutor {
        calls: Mutex::new(Vec::new()),
    });
    let inspector = CommandComposeInspector::new(Arc::clone(&executor));
    let project = ComposeProjectRef::new("soma", "/srv/soma/compose.yaml").unwrap();
    assert!(
        inspector
            .status(
                &host(),
                &project,
                Some("bad service"),
                deadline(),
                &CancellationToken::new()
            )
            .await
            .is_err()
    );
    assert!(executor.calls.lock().unwrap().is_empty());
}

#[test]
fn nonzero_and_truncated_outputs_fail_closed() {
    let host = host();
    assert!(matches!(
        checked_output(
            &host,
            CommandOutput::new(Vec::new(), b"bad".to_vec(), Some(1), false)
        ),
        Err(InfraError::CommandFailed { .. })
    ));
    assert!(matches!(
        checked_output(
            &host,
            CommandOutput::new(b"[]".to_vec(), Vec::new(), Some(0), true)
        ),
        Err(InfraError::Parse { .. })
    ));
}
