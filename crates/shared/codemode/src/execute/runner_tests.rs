use std::sync::Arc;

use serial_test::serial;

use crate::CodeModeConfig;
use crate::host::NoopHost;
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
