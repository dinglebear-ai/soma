use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use serial_test::serial;

use crate::CodeModeConfig;
use crate::host::{
    CodeModeHost, ExecCtx, HostFuture, NoopHost, ResolvedSnippet, ToolCallOutcome, ToolsRender,
};
use crate::pool::{PoolConfig, RunnerDisposition, RunnerPool, RunnerSpawn};
use crate::types::{CodeModeCaller, CodeModeSurface, ToolDescriptor, ToolScope};

use super::runner::{SubprocessExecution, execute_in_subprocess};

#[derive(Debug)]
struct DelayedToolHost {
    delay: Duration,
}

impl CodeModeHost for DelayedToolHost {
    fn list_tools<'a>(
        &'a self,
        caller: &'a CodeModeCaller,
        surface: CodeModeSurface,
        scope: &'a ToolScope,
        include_snippets: bool,
        use_cache: bool,
    ) -> HostFuture<'a, Result<ToolsRender, crate::ToolError>> {
        let _ = (caller, surface, scope, include_snippets, use_cache);
        let entries: Arc<[ToolDescriptor]> = Arc::from([ToolDescriptor::tool(
            "stub",
            "slow",
            "delayed test tool",
            Some(json!({"type": "object"})),
            None,
        )]);
        Box::pin(async move {
            Ok(ToolsRender {
                fingerprint: "delayed".to_string(),
                entries,
                catalog_json: Arc::from("[]"),
                serialized_size: 2,
            })
        })
    }

    fn call_tool<'a>(
        &'a self,
        _id: &'a str,
        _params: Value,
        _caller: &'a CodeModeCaller,
        _surface: CodeModeSurface,
        _scope: &'a ToolScope,
        _ctx: ExecCtx,
    ) -> HostFuture<'a, Result<ToolCallOutcome, crate::ToolError>> {
        let delay = self.delay;
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            Ok(ToolCallOutcome {
                value: json!({"late": true}),
                ui: None,
            })
        })
    }

    fn resolve_snippet<'a>(
        &'a self,
        name: &'a str,
        _input: Value,
    ) -> HostFuture<'a, Result<ResolvedSnippet, crate::ToolError>> {
        Box::pin(async move {
            Err(crate::ToolError::UnknownInstance {
                message: format!("unknown snippet {name}"),
                valid: Vec::new(),
            })
        })
    }
}

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
    let config = CodeModeConfig {
        timeout_ms: 5_000,
        ..CodeModeConfig::default()
    };

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
    let config = CodeModeConfig {
        timeout_ms: 5_000,
        ..CodeModeConfig::default()
    };

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

#[tokio::test]
#[serial(code_mode_soma_home)]
async fn external_tool_timeout_leaves_budget_for_runner_acknowledgement() {
    let script = r#"IFS= read -r _
printf '%s\n' '{"type":"tool_call","seq":1,"id":"stub::slow","params":{}}'
IFS= read -r reply
case "$reply" in
  *'"type":"tool_error"'*) ;;
  *) exit 23 ;;
esac
printf '%s\n' '{"type":"done","result":{"state":"json","value":{"acknowledged":true}}}'
sleep 60
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
    let host = DelayedToolHost {
        delay: Duration::from_secs(5),
    };
    let config = CodeModeConfig {
        timeout_ms: 2_000,
        ..CodeModeConfig::default()
    };

    let outcome = execute_in_subprocess(SubprocessExecution {
        host: Some(&host),
        runner_pool: Some(&pool),
        code: "async () => null",
        caller: CodeModeCaller::trusted_local("test"),
        surface: CodeModeSurface::Cli,
        config,
        scope: ToolScope::All,
        execution_id: None,
        ui_capture: Arc::new(std::sync::Mutex::new(None)),
    })
    .await
    .expect("tool timeout must leave enough budget for Done acknowledgement");

    assert_eq!(
        outcome.raw_response.result,
        Some(json!({"acknowledged": true}))
    );
    assert_eq!(outcome.raw_response.calls.len(), 1);
    assert!(outcome.raw_response.calls[0].result.is_none());
}

#[tokio::test]
#[serial(code_mode_soma_home)]
async fn runner_is_evicted_after_post_tool_settlement_grace() {
    let script = r#"IFS= read -r _
printf '%s\n' '{"type":"tool_call","seq":1,"id":"stub::slow","params":{}}'
IFS= read -r _
sleep 60
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
    let host = DelayedToolHost {
        delay: Duration::ZERO,
    };
    let config = CodeModeConfig {
        timeout_ms: 15_000,
        ..CodeModeConfig::default()
    };
    let started = std::time::Instant::now();

    let error = execute_in_subprocess(SubprocessExecution {
        host: Some(&host),
        runner_pool: Some(&pool),
        code: "async () => null",
        caller: CodeModeCaller::trusted_local("test"),
        surface: CodeModeSurface::Cli,
        config,
        scope: ToolScope::All,
        execution_id: None,
        ui_capture: Arc::new(std::sync::Mutex::new(None)),
    })
    .await
    .unwrap_err();

    assert_eq!(error.kind(), "timeout");
    assert!(error.to_string().contains("did not settle"));
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "settlement watchdog must not consume the 15-second outer timeout"
    );
}

#[tokio::test]
#[serial(code_mode_soma_home)]
async fn outer_timeout_remains_authoritative_during_settlement() {
    let script = r#"IFS= read -r _
printf '%s\n' '{"type":"tool_call","seq":1,"id":"stub::slow","params":{}}'
IFS= read -r _
sleep 60
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
    let host = DelayedToolHost {
        delay: Duration::ZERO,
    };
    let config = CodeModeConfig {
        timeout_ms: 700,
        ..CodeModeConfig::default()
    };

    let error = execute_in_subprocess(SubprocessExecution {
        host: Some(&host),
        runner_pool: Some(&pool),
        code: "async () => null",
        caller: CodeModeCaller::trusted_local("test"),
        surface: CodeModeSurface::Cli,
        config,
        scope: ToolScope::All,
        execution_id: None,
        ui_capture: Arc::new(std::sync::Mutex::new(None)),
    })
    .await
    .unwrap_err();

    assert_eq!(error.kind(), "timeout");
    assert_eq!(error.user_message(), "Code Mode execution timed out");
}
