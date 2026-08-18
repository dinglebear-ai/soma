use std::sync::Arc;

use serial_test::serial;

use crate::CodeModeConfig;
use crate::host::NoopHost;
use crate::pool::{PoolConfig, RunnerDisposition, RunnerPool, RunnerSpawn};
use crate::types::{CodeModeCaller, CodeModeSurface, ToolScope};

use super::runner::{SubprocessExecution, execute_in_subprocess};

#[tokio::test]
#[serial(code_mode_soma_home, code_mode_runner_exe_env)]
async fn subprocess_runner_executes_plain_code_without_host() {
    let outcome = execute_in_subprocess::<NoopHost>(SubprocessExecution {
        host: None,
        runner_pool: None,
        code: "async () => ({ answer: 42 })",
        caller: CodeModeCaller::trusted_local("test"),
        surface: CodeModeSurface::Cli,
        config: CodeModeConfig::default(),
        scope: ToolScope::All,
        execution_id: None,
        ui_capture: Arc::new(std::sync::Mutex::new(None)),
    })
    .await
    .unwrap();

    assert_eq!(
        outcome.raw_response.result,
        Some(serde_json::json!({"answer": 42}))
    );
    assert!(outcome.raw_response.calls.is_empty());
}

#[tokio::test]
#[serial(code_mode_soma_home)]
async fn subprocess_runner_retries_idle_runner_exit_once_on_fresh_process() {
    let marker_dir = tempfile::tempdir().unwrap();
    let marker = marker_dir.path().join("first-run");
    let script = format!(
        r#"marker="{}"
if [ ! -f "$marker" ]; then
  : > "$marker"
  IFS= read -r _
  exit 0
fi
IFS= read -r _
printf '%s\n' '{{"type":"done","result":{{"state":"json","value":{{"retried":true}}}}}}'
sleep 60
"#,
        marker.display()
    );
    let pool = RunnerPool::new(
        PoolConfig {
            size: 1,
            recycle_after: 10,
            max_overflow: 1,
        },
        RunnerSpawn {
            program: "/bin/sh".into(),
            args: vec!["-c".to_string(), script],
        },
    );
    let mut config = CodeModeConfig::default();
    config.timeout_ms = 5_000;

    let outcome = execute_in_subprocess::<NoopHost>(SubprocessExecution {
        host: None,
        runner_pool: Some(&pool),
        code: "async () => 1",
        caller: CodeModeCaller::trusted_local("test"),
        surface: CodeModeSurface::Cli,
        config,
        scope: ToolScope::All,
        execution_id: None,
        ui_capture: Arc::new(std::sync::Mutex::new(None)),
    })
    .await
    .unwrap();

    assert_eq!(
        outcome.raw_response.result,
        Some(serde_json::json!({"retried": true}))
    );
}

#[tokio::test]
#[serial(code_mode_soma_home)]
async fn subprocess_runner_never_replays_after_protocol_activity() {
    let marker_dir = tempfile::tempdir().unwrap();
    let counter = marker_dir.path().join("starts");
    let script = format!(
        r#"counter="{}"
printf 'x\n' >> "$counter"
IFS= read -r _
printf '%s\n' '{{"type":"tool_call","seq":0,"id":"missing::tool","params":{{}}}}'
IFS= read -r _
exit 0
"#,
        counter.display()
    );
    let pool = RunnerPool::new(
        PoolConfig {
            size: 1,
            recycle_after: 10,
            max_overflow: 1,
        },
        RunnerSpawn {
            program: "/bin/sh".into(),
            args: vec!["-c".to_string(), script],
        },
    );
    let mut config = CodeModeConfig::default();
    config.timeout_ms = 5_000;

    let error = execute_in_subprocess::<NoopHost>(SubprocessExecution {
        host: None,
        runner_pool: Some(&pool),
        code: "async () => 1",
        caller: CodeModeCaller::trusted_local("test"),
        surface: CodeModeSurface::Cli,
        config,
        scope: ToolScope::All,
        execution_id: None,
        ui_capture: Arc::new(std::sync::Mutex::new(None)),
    })
    .await
    .unwrap_err();

    assert_eq!(error.kind(), "server_error");
    assert_eq!(std::fs::read_to_string(counter).unwrap().lines().count(), 1);
}

#[tokio::test]
#[serial(code_mode_soma_home)]
async fn subprocess_runner_reuses_process_after_execution_error() {
    let script = r#"while IFS= read -r _; do
  printf '%s\n' '{"type":"error","kind":"bad_request","message":"expected"}'
done
"#;
    let pool = RunnerPool::new(
        PoolConfig {
            size: 1,
            recycle_after: 10,
            max_overflow: 0,
        },
        RunnerSpawn {
            program: "/bin/sh".into(),
            args: vec!["-c".to_string(), script.to_string()],
        },
    );
    let first = pool.checkout().await.unwrap();
    let first_pid = first.handle.as_ref().and_then(|handle| handle.child_pid);
    pool.release(first, RunnerDisposition::Reuse).await;

    let error = execute_in_subprocess::<NoopHost>(SubprocessExecution {
        host: None,
        runner_pool: Some(&pool),
        code: "async () => 1",
        caller: CodeModeCaller::trusted_local("test"),
        surface: CodeModeSurface::Cli,
        config: CodeModeConfig::default(),
        scope: ToolScope::All,
        execution_id: None,
        ui_capture: Arc::new(std::sync::Mutex::new(None)),
    })
    .await
    .unwrap_err();
    assert_eq!(error.kind(), "bad_request");

    let second = pool.checkout().await.unwrap();
    let second_pid = second.handle.as_ref().and_then(|handle| handle.child_pid);
    assert_eq!(first_pid, second_pid);
}
