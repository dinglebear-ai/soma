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
use crate::{normalize_user_code, CodeModeConfig, ToolError};

use super::budget::RunBudget;
use super::proxy::{build_proxy, load_entries};
use super::tool_dispatch::{handle_tool_call, ToolCallContext};
use super::{finish_response, CodeModeExecutionOutcome};

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
    let proxy = build_proxy(&entries, config.semantic_search.blend_weight)?;
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
    let deadline = tokio::time::Instant::now() + Duration::from_millis(config.timeout_ms.max(1));
    write_with_deadline(
        &mut lease.handle_mut()?.stdin,
        &CodeModeRunnerInput::Start {
            code: normalize_user_code(request.code),
            proxy,
        },
        deadline,
    )
    .await?;

    let mut calls = Vec::new();
    let mut step_ordinals: HashMap<u64, (u64, String)> = HashMap::new();
    let mut next_step_ordinal = 0u64;
    let artifact_run_id = request
        .execution_id
        .as_deref()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| ulid::Ulid::generate().to_string());
    let artifact_store = ArtifactStore::new(artifact_run_id)?;
    crate::artifacts::prune::prune_old_runs(&crate::soma_home().join("code-mode-artifacts"), 256)
        .await
        .map_err(|err| ToolError::internal_message(format!("prune artifacts: {err}")))?;
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
        let output = next_output(lease.handle_mut()?, deadline).await?;
        match output {
            CodeModeRunnerOutput::ToolCall { seq, id, params } => {
                let result = handle_tool_call(&mut tool_ctx, &mut budget, seq, id, params).await;
                settle(seq, result, &mut lease.handle_mut()?.stdin, deadline).await?;
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
                return Err(ToolError::Sdk {
                    sdk_kind: kind,
                    message,
                });
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

async fn next_output(
    runner: &mut crate::pool::RunnerHandle,
    deadline: tokio::time::Instant,
) -> Result<CodeModeRunnerOutput, ToolError> {
    match tokio::time::timeout_at(deadline, runner.lines.next()).await {
        Ok(Some(Ok(line))) => decode_runner_output(&line),
        Ok(Some(Err(error))) => Err(ToolError::internal_message(format!(
            "failed to read runner output: {error}"
        ))),
        Ok(None) => Err(ToolError::internal_message(
            "runner exited before completion",
        )),
        Err(_) => {
            terminate_code_mode_runner(&mut runner.child, runner.child_pid).await;
            Err(ToolError::Sdk {
                sdk_kind: "timeout".to_string(),
                message: "Code Mode execution timed out".to_string(),
            })
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
