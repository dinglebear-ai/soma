use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use serde_json::Value;

use crate::artifacts::ArtifactStore;
use crate::host::{CodeModeHost, ExecCtx, StepDecision};
use crate::local_provider::{dispatch_local_provider, parse_local_provider_call};
use crate::preamble::{
    generate_discovery_js, generate_js_proxy_from_catalog, generate_local_provider_js,
};
use crate::protocol::{CodeModeRunnerInput, CodeModeRunnerOutput};
use crate::runner_io::{decode_runner_output, terminate_code_mode_runner, write_runner_input};
use crate::types::{
    CodeModeCaller, CodeModeExecutedCall, CodeModeExecutionResponse, CodeModeSurface,
    ToolDescriptor, ToolScope, UiLink,
};
use crate::{normalize_user_code, CodeModeConfig, ToolError};

use super::{finish_response, CodeModeExecutionOutcome};

pub(crate) struct SubprocessExecution<'a, H: CodeModeHost> {
    pub(crate) host: Option<&'a H>,
    pub(crate) code: &'a str,
    pub(crate) caller: CodeModeCaller,
    pub(crate) surface: CodeModeSurface,
    pub(crate) config: CodeModeConfig,
    pub(crate) scope: ToolScope,
    pub(crate) execution_id: Option<Arc<str>>,
    pub(crate) ui_capture: Arc<std::sync::Mutex<Option<UiLink>>>,
}

struct ToolCallContext<'a, H: CodeModeHost> {
    host: Option<&'a H>,
    entries: &'a [ToolDescriptor],
    caller: &'a CodeModeCaller,
    surface: CodeModeSurface,
    scope: &'a ToolScope,
    execution_id: &'a Option<Arc<str>>,
    ui_capture: &'a Arc<std::sync::Mutex<Option<UiLink>>>,
    calls: &'a mut Vec<CodeModeExecutedCall>,
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
    let proxy = build_proxy(&entries, config.semantic_search.blend_weight)?;
    let mut runner = crate::pool::RunnerHandle::spawn(&crate::pool::RunnerSpawn::current_exe()?)?;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(config.timeout_ms.max(1));
    write_with_deadline(
        &mut runner.stdin,
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
        .unwrap_or_else(|| ulid::Ulid::new().to_string());
    let artifact_store = ArtifactStore::new(artifact_run_id);
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
        let output = next_output(&mut runner, deadline).await?;
        match output {
            CodeModeRunnerOutput::ToolCall { seq, id, params } => {
                let result = handle_tool_call(&mut tool_ctx, seq, id, params).await;
                settle(seq, result, &mut runner.stdin, deadline).await?;
            }
            CodeModeRunnerOutput::ArtifactWrite {
                seq,
                path,
                content,
                content_type,
            } => {
                let result = artifact_store
                    .write_text(&path, &content, content_type.as_deref())
                    .await
                    .and_then(to_value);
                settle(seq, result, &mut runner.stdin, deadline).await?;
            }
            CodeModeRunnerOutput::SnippetResolve { seq, name, input } => {
                let result = resolve_snippet(request.host, name, input).await;
                match result {
                    Ok((code, input)) => {
                        write_with_deadline(
                            &mut runner.stdin,
                            &CodeModeRunnerInput::SnippetResolved { seq, code, input },
                            deadline,
                        )
                        .await?;
                    }
                    Err(error) => write_error(seq, error, &mut runner.stdin, deadline).await?,
                }
            }
            CodeModeRunnerOutput::StepBegin { seq, name } => {
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
                            &mut runner.stdin,
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
                            &mut runner.stdin,
                            &CodeModeRunnerInput::StepDecision { seq, replay: None },
                            deadline,
                        )
                        .await?;
                    }
                    StepDecision::Error { kind, message } => {
                        write_with_deadline(
                            &mut runner.stdin,
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
                            &mut runner.stdin,
                            &CodeModeRunnerInput::StepRecorded { seq },
                            deadline,
                        )
                        .await?;
                    }
                    Err(error) => write_error(seq, error, &mut runner.stdin, deadline).await?,
                }
            }
            CodeModeRunnerOutput::Done { result, logs } => {
                runner.stderr.flush_settle().await;
                let mut logs = logs;
                logs.extend(runner.stderr.take_since_and_clear(0).await);
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
                return finish_response(raw, &config);
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

async fn load_entries<H: CodeModeHost>(
    host: Option<&H>,
    caller: &CodeModeCaller,
    surface: CodeModeSurface,
    scope: &ToolScope,
) -> Result<Vec<ToolDescriptor>, ToolError> {
    match host {
        Some(host) => Ok(host
            .list_tools(caller, surface, scope, true, true)
            .await?
            .entries
            .iter()
            .filter(|entry| scope.allows(&entry.id))
            .cloned()
            .collect()),
        None => Ok(Vec::new()),
    }
}

pub(crate) fn build_proxy(
    entries: &[ToolDescriptor],
    blend_weight: f32,
) -> Result<String, ToolError> {
    let values = entries
        .iter()
        .map(|entry| serde_json::to_value(entry).map_err(serialize_error))
        .collect::<Result<Vec<_>, _>>()?;
    let mut proxy = String::new();
    proxy.push_str(generate_local_provider_js());
    proxy.push_str(
        &generate_discovery_js(&values, blend_weight).map_err(ToolError::internal_message)?,
    );
    proxy.push_str(&generate_js_proxy_from_catalog(entries).map_err(ToolError::internal_message)?);
    proxy.push_str(
        r#"
codemode.run = (name, input = {}) => globalThis.__somaRunSnippet(name, input);
codemode.step = (name, fn) => globalThis.__somaCodemodeStep(name, fn);
codemode.search = async (query = "") => {
  const q = String(query || "").toLowerCase();
  return globalThis.__codemodeDiscovery.filter((entry) => JSON.stringify(entry).toLowerCase().includes(q));
};
codemode.describe = async (query = "") => ({
  tools: (await codemode.search(query)).map((entry) => ({
    id: entry.id,
    signature: entry.signature,
    dts: entry.dts,
    description: entry.description
  }))
});
"#,
    );
    Ok(proxy)
}

async fn handle_tool_call<H: CodeModeHost>(
    ctx: &mut ToolCallContext<'_, H>,
    seq: u64,
    id: String,
    params: Value,
) -> Result<Value, ToolError> {
    let result = if let Some(call) = parse_local_provider_call(&id, params.clone())? {
        if !local_providers_allowed(ctx.caller, ctx.scope) {
            Err(ToolError::Forbidden {
                message: format!("Code Mode local provider `{id}` is not available in this scope"),
                required_scopes: vec!["soma:admin".to_string()],
            })
        } else {
            dispatch_local_provider(call).await
        }
    } else {
        let host = ctx.host.ok_or_else(|| unknown_tool(&id, ctx.entries))?;
        let descriptor = ctx
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| unknown_tool(&id, ctx.entries))?;
        let outcome = call_host_tool_with_ctx(
            host,
            descriptor,
            params.clone(),
            ctx.caller,
            ctx.surface,
            ctx.scope,
            ExecCtx {
                seq,
                execution_id: ctx.execution_id.clone(),
                step_ordinal: None,
            },
        )
        .await?;
        if let Some(ui) = outcome.ui.clone() {
            if let Ok(mut guard) = ctx.ui_capture.lock() {
                *guard = Some(ui);
            }
        }
        Ok(outcome.value)
    };
    match &result {
        Ok(value) => ctx.calls.push(CodeModeExecutedCall {
            id,
            params: Some(params),
            result: Some(value.clone()),
        }),
        Err(_) => ctx.calls.push(CodeModeExecutedCall {
            id,
            params: Some(params),
            result: None,
        }),
    }
    result
}

async fn call_host_tool_with_ctx<H: CodeModeHost>(
    host: &H,
    descriptor: &ToolDescriptor,
    params: Value,
    caller: &CodeModeCaller,
    surface: CodeModeSurface,
    scope: &ToolScope,
    ctx: ExecCtx,
) -> Result<crate::host::ToolCallOutcome, ToolError> {
    if !scope.allows(&descriptor.id) {
        return Err(ToolError::Forbidden {
            message: format!("Code Mode scope does not allow `{}`", descriptor.id),
            required_scopes: vec![descriptor.namespace.clone()],
        });
    }
    crate::schema::validate_code_mode_params_against_schema(&params, descriptor.schema.as_ref())?;
    host.call_tool(&descriptor.id, params, caller, surface, scope, ctx)
        .await
}

pub(crate) fn local_providers_allowed(caller: &CodeModeCaller, scope: &ToolScope) -> bool {
    matches!(scope, ToolScope::All)
        && (caller.capabilities.admin || caller.capabilities.trusted_local)
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

fn unknown_tool(id: &str, entries: &[ToolDescriptor]) -> ToolError {
    ToolError::UnknownAction {
        message: format!("unknown Code Mode tool `{id}`"),
        valid: entries.iter().map(|entry| entry.id.clone()).collect(),
        hint: Some(crate::broker::code_mode_unknown_tool_hint()),
    }
}

fn to_value<T: serde::Serialize>(value: T) -> Result<Value, ToolError> {
    serde_json::to_value(value).map_err(serialize_error)
}

fn serialize_error(error: serde_json::Error) -> ToolError {
    ToolError::internal_message(format!("failed to serialize Code Mode value: {error}"))
}
