use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use serde_json::Value;

use crate::artifacts::ArtifactStore;
use crate::host::{CodeModeHost, ExecCtx, StepDecision};
use crate::pool::{PoolConfig, RunnerDisposition, RunnerPool, RunnerSpawn};
use crate::protocol::{CodeModeRunnerInput, CodeModeRunnerOutput};
use crate::runner_io::{decode_runner_output, terminate_code_mode_runner, write_runner_input};
use crate::types::{CodeModeCaller, CodeModeExecutionResponse, CodeModeSurface, ToolScope, UiLink};
use crate::{CodeModeConfig, ToolError, normalize_user_code};

use super::budget::RunBudget;
use super::proxy::{build_proxy, load_entries};
use super::settlement::{RUNNER_SETTLEMENT_GRACE, SettlementWatch, external_tool_deadline};
use super::tool_dispatch::{ToolCallContext, handle_tool_call};
use super::{CodeModeExecutionOutcome, finish_response};

pub(crate) struct SubprocessExecution<'a, H: CodeModeHost> {
    pub(crate) host: Option<&'a H>,
    pub(crate) runner_pool: Option<&'a RunnerPool>,
    pub(crate) code: &'a str,
    pub(crate) caller: CodeModeCaller,
    pub(crate) surface: CodeModeSurface,
    pub(crate) config: CodeModeConfig,
    pub(crate) scope: ToolScope,
    pub(crate) execution_id: Option<Arc<str>>,
    pub(crate) ui_capture: Arc<std::sync::Mutex<Option<UiLink>>>,
}

pub(crate) async fn execute_in_subprocess<H: CodeModeHost>(
    request: SubprocessExecution<'_, H>,
) -> Result<CodeModeExecutionOutcome, ToolError> {
    let entries = load_entries(
        request.host,
        &request.caller,
        request.surface,
        &request.scope,
    )
    .await?;
    let config = request.config;
    let mut budget = RunBudget::new(&config);
    let start_input = CodeModeRunnerInput::Start {
        code: normalize_user_code(request.code),
        proxy: build_proxy(&entries, config.semantic_search.blend_weight)?,
    };
    let fallback_pool;
    let pool = if let Some(pool) = request.runner_pool {
        pool
    } else {
        fallback_pool = RunnerPool::new(
            PoolConfig {
                size: 0,
                recycle_after: 1,
                max_overflow: 1,
            },
            RunnerSpawn::current_exe()?,
        );
        &fallback_pool
    };
    let mut lease = pool.checkout().await?;
    let mut deadline =
        tokio::time::Instant::now() + Duration::from_millis(config.timeout_ms.max(1));
    write_with_deadline(&mut lease.handle_mut()?.stdin, &start_input, deadline).await?;
    let mut saw_protocol_activity = false;
    let mut replayed_on_fresh_runner = false;
    let mut settlement_watch: Option<SettlementWatch> = None;

    let mut calls = Vec::new();
    let mut step_ordinals: HashMap<u64, (u64, String)> = HashMap::new();
    let mut next_step_ordinal = 0u64;
    let artifact_run_id = request
        .execution_id
        .as_deref()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| ulid::Ulid::generate().to_string());
    let artifact_store = ArtifactStore::new(artifact_run_id)?
        .with_max_bytes(crate::config::effective_artifact_max_bytes(&config))
        .with_retention_limits(
            crate::config::effective_artifact_retention_runs(&config),
            crate::config::effective_artifact_max_store_bytes(&config),
        );
    let mut tool_ctx = ToolCallContext {
        host: request.host,
        entries: &entries,
        caller: &request.caller,
        surface: request.surface,
        scope: &request.scope,
        execution_id: &request.execution_id,
        ui_capture: &request.ui_capture,
        calls: &mut calls,
    };

    loop {
        let active_settlement_watch = settlement_watch;
        let read_deadline = active_settlement_watch.map_or(deadline, |watch| watch.deadline);
        let output = match next_output(lease.handle_mut()?, read_deadline).await {
            Ok(output) => {
                saw_protocol_activity = true;
                // Any protocol activity proves the runner is alive and ends the
                // prior post-result settlement watch. A new watch is armed only
                // after the next ToolResult/ToolError is written successfully.
                settlement_watch = None;
                output
            }
            Err(NextOutputError::RunnerExited)
                if !saw_protocol_activity && !replayed_on_fresh_runner =>
            {
                drop(lease);
                tracing::warn!(
                    surface = "dispatch",
                    service = "code_mode",
                    action = "pool.retry_fresh",
                    "Code Mode runner exited before protocol activity; retrying once on a fresh runner"
                );
                lease = pool.checkout_fresh().await?;
                deadline =
                    tokio::time::Instant::now() + Duration::from_millis(config.timeout_ms.max(1));
                write_with_deadline(&mut lease.handle_mut()?.stdin, &start_input, deadline).await?;
                replayed_on_fresh_runner = true;
                settlement_watch = None;
                continue;
            }
            Err(NextOutputError::TimedOut) => {
                return Err(match active_settlement_watch {
                    Some(watch) if watch.grace_limited => settlement_timeout_error(),
                    _ => execution_timeout_error(),
                });
            }
            Err(error) => return Err(error.into_tool_error()),
        };
        match output {
            CodeModeRunnerOutput::ToolCall { seq, id, params } => {
                let tool_deadline = external_tool_deadline(tokio::time::Instant::now(), deadline);
                let result =
                    handle_tool_call(&mut tool_ctx, &mut budget, seq, id, params, tool_deadline)
                        .await;
                settle(seq, result, &mut lease.handle_mut()?.stdin, deadline).await?;
                settlement_watch =
                    Some(SettlementWatch::new(tokio::time::Instant::now(), deadline));
            }
            CodeModeRunnerOutput::ArtifactWrite {
                seq,
                path,
                content,
                content_type,
            } => {
                let result = match budget.record_operation("artifact write") {
                    Ok(()) => artifact_store
                        .write_text(&path, &content, content_type.as_deref())
                        .await
                        .and_then(to_value),
                    Err(error) => Err(error),
                };
                settle(seq, result, &mut lease.handle_mut()?.stdin, deadline).await?;
            }
            CodeModeRunnerOutput::SnippetResolve { seq, name, input } => {
                let result = match budget.record_operation("snippet resolve") {
                    Ok(()) => resolve_snippet(request.host, name, input).await,
                    Err(error) => Err(error),
                };
                match result {
                    Ok((code, input)) => {
                        write_with_deadline(
                            &mut lease.handle_mut()?.stdin,
                            &CodeModeRunnerInput::SnippetResolved { seq, code, input },
                            deadline,
                        )
                        .await?;
                    }
                    Err(error) => {
                        write_error(seq, error, &mut lease.handle_mut()?.stdin, deadline).await?
                    }
                }
            }
            CodeModeRunnerOutput::StepBegin { seq, name } => {
                if let Err(error) = budget.record_operation("step") {
                    write_error(seq, error, &mut lease.handle_mut()?.stdin, deadline).await?;
                    continue;
                }
                let ordinal = next_step_ordinal;
                next_step_ordinal = next_step_ordinal.saturating_add(1);
                step_ordinals.insert(seq, (ordinal, name.clone()));
                let decision = decide_step(
                    request.host,
                    request.execution_id.clone(),
                    seq,
                    ordinal,
                    &name,
                )
                .await;
                match decision {
                    StepDecision::Replay(value) => {
                        write_with_deadline(
                            &mut lease.handle_mut()?.stdin,
                            &CodeModeRunnerInput::StepDecision {
                                seq,
                                replay: Some(value),
                            },
                            deadline,
                        )
                        .await?;
                    }
                    StepDecision::Execute => {
                        write_with_deadline(
                            &mut lease.handle_mut()?.stdin,
                            &CodeModeRunnerInput::StepDecision { seq, replay: None },
                            deadline,
                        )
                        .await?;
                    }
                    StepDecision::Error { kind, message } => {
                        write_with_deadline(
                            &mut lease.handle_mut()?.stdin,
                            &CodeModeRunnerInput::ToolError { seq, kind, message },
                            deadline,
                        )
                        .await?;
                    }
                }
            }
            CodeModeRunnerOutput::StepResult { seq, value } => {
                let result = record_step(
                    request.host,
                    request.execution_id.clone(),
                    seq,
                    &value,
                    &step_ordinals,
                )
                .await;
                match result {
                    Ok(()) => {
                        write_with_deadline(
                            &mut lease.handle_mut()?.stdin,
                            &CodeModeRunnerInput::StepRecorded { seq },
                            deadline,
                        )
                        .await?;
                    }
                    Err(error) => {
                        write_error(seq, error, &mut lease.handle_mut()?.stdin, deadline).await?
                    }
                }
            }
            CodeModeRunnerOutput::Done { result, logs } => {
                lease.handle_mut()?.stderr.flush_settle().await;
                let mut logs = logs;
                logs.extend(lease.handle_mut()?.stderr.take_since_and_clear(0).await);
                let logs = budget.cap_logs(logs);
                let raw = CodeModeExecutionResponse {
                    result: result.into_response_result(),
                    calls,
                    logs,
                    error: None,
                    ui: request
                        .ui_capture
                        .lock()
                        .ok()
                        .and_then(|guard| guard.clone()),
                };
                let response = finish_response(raw, &config);
                let handle = lease.handle_mut()?;
                handle.success_count = handle.success_count.saturating_add(1);
                let disposition = RunnerDisposition::from_success_count(
                    handle.success_count,
                    pool.config().recycle_after,
                );
                pool.release(lease, disposition).await;
                return response;
            }
            CodeModeRunnerOutput::Error { kind, message } => {
                let error = ToolError::Sdk {
                    sdk_kind: kind,
                    message,
                };
                pool.release(lease, RunnerDisposition::Reuse).await;
                return Err(error);
            }
        }
    }
}

async fn resolve_snippet<H: CodeModeHost>(
    host: Option<&H>,
    name: String,
    input: Value,
) -> Result<(String, Value), ToolError> {
    let host = host.ok_or_else(|| ToolError::UnknownInstance {
        message: format!("unknown Code Mode snippet `{name}`"),
        valid: Vec::new(),
    })?;
    let resolved = host.resolve_snippet(&name, input).await?;
    Ok((resolved.code, resolved.input))
}

async fn decide_step<H: CodeModeHost>(
    host: Option<&H>,
    execution_id: Option<Arc<str>>,
    seq: u64,
    ordinal: u64,
    name: &str,
) -> StepDecision {
    match host {
        Some(host) => {
            host.decide_step(
                ExecCtx {
                    seq,
                    execution_id,
                    step_ordinal: Some(ordinal),
                },
                name,
            )
            .await
        }
        None => StepDecision::Execute,
    }
}

async fn record_step<H: CodeModeHost>(
    host: Option<&H>,
    execution_id: Option<Arc<str>>,
    seq: u64,
    value: &Value,
    step_ordinals: &HashMap<u64, (u64, String)>,
) -> Result<(), ToolError> {
    let Some(host) = host else {
        return Ok(());
    };
    let (ordinal, name) = step_ordinals
        .get(&seq)
        .ok_or_else(|| ToolError::internal_message("runner returned an unknown step result seq"))?;
    host.record_step(
        ExecCtx {
            seq,
            execution_id,
            step_ordinal: Some(*ordinal),
        },
        name,
        value,
    )
    .await
}

fn execution_timeout_error() -> ToolError {
    ToolError::Sdk {
        sdk_kind: "timeout".to_string(),
        message: "Code Mode execution timed out".to_string(),
    }
}

fn settlement_timeout_error() -> ToolError {
    tracing::warn!(
        surface = "dispatch",
        service = "code_mode",
        action = "codemode.settlement",
        kind = "runner_settlement_timeout",
        grace_ms = RUNNER_SETTLEMENT_GRACE.as_millis(),
        "Code Mode runner failed to settle after a tool result was delivered"
    );
    ToolError::Sdk {
        sdk_kind: "timeout".to_string(),
        message: format!(
            "Code Mode runner did not settle within {}ms after a tool call completed",
            RUNNER_SETTLEMENT_GRACE.as_millis()
        ),
    }
}

enum NextOutputError {
    RunnerExited,
    TimedOut,
    Tool(ToolError),
}

impl NextOutputError {
    fn into_tool_error(self) -> ToolError {
        match self {
            Self::RunnerExited => ToolError::Sdk {
                sdk_kind: "server_error".to_string(),
                message: "Code Mode runner exited before completion".to_string(),
            },
            Self::TimedOut => execution_timeout_error(),
            Self::Tool(error) => error,
        }
    }
}

async fn next_output(
    runner: &mut crate::pool::RunnerHandle,
    deadline: tokio::time::Instant,
) -> Result<CodeModeRunnerOutput, NextOutputError> {
    match tokio::time::timeout_at(deadline, runner.lines.next()).await {
        Ok(Some(Ok(line))) => decode_runner_output(&line).map_err(NextOutputError::Tool),
        Ok(Some(Err(error))) => Err(NextOutputError::Tool(ToolError::internal_message(format!(
            "failed to read runner output: {error}"
        )))),
        Ok(None) => Err(NextOutputError::RunnerExited),
        Err(_) => {
            terminate_code_mode_runner(&mut runner.child, runner.child_pid).await;
            Err(NextOutputError::TimedOut)
        }
    }
}

async fn settle<W: tokio::io::AsyncWriteExt + Unpin>(
    seq: u64,
    result: Result<Value, ToolError>,
    writer: &mut W,
    deadline: tokio::time::Instant,
) -> Result<(), ToolError> {
    match result {
        Ok(result) => {
            write_with_deadline(
                writer,
                &CodeModeRunnerInput::ToolResult { seq, result },
                deadline,
            )
            .await
        }
        Err(error) => write_error(seq, error, writer, deadline).await,
    }
}

async fn write_error<W: tokio::io::AsyncWriteExt + Unpin>(
    seq: u64,
    error: ToolError,
    writer: &mut W,
    deadline: tokio::time::Instant,
) -> Result<(), ToolError> {
    write_with_deadline(
        writer,
        &CodeModeRunnerInput::ToolError {
            seq,
            kind: error.kind().to_string(),
            message: error.user_message().to_string(),
        },
        deadline,
    )
    .await
}

async fn write_with_deadline<W: tokio::io::AsyncWriteExt + Unpin>(
    writer: &mut W,
    input: &CodeModeRunnerInput,
    deadline: tokio::time::Instant,
) -> Result<(), ToolError> {
    tokio::time::timeout_at(deadline, write_runner_input(writer, input))
        .await
        .map_err(|_| ToolError::Sdk {
            sdk_kind: "timeout".to_string(),
            message: "Code Mode runner write timed out".to_string(),
        })?
}

fn to_value<T: serde::Serialize>(value: T) -> Result<Value, ToolError> {
    serde_json::to_value(value).map_err(serialize_error)
}

fn serialize_error(error: serde_json::Error) -> ToolError {
    ToolError::internal_message(format!("failed to serialize Code Mode value: {error}"))
}
