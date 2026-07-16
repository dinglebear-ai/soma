use serde_json::Value;

use crate::protocol::{
    CodeModeRunnerInput, CodeModeRunnerOutput, CodeModeRunnerResult, CODE_MODE_STACK_SIZE_LIMIT,
    RUNNER_STATE,
};
use crate::runner::jail::reset_execution_jail;
use crate::runner::js_args::{javy_type_error, json_arg, optional_string_arg, required_string_arg};
use crate::runner::limits::MEMORY_LIMIT_BYTES;
use crate::wrapper::code_mode_runner_script;

pub enum RunnerLoopOutcome {
    Completed,
    InputClosed,
}

pub struct RunnerError {
    pub kind: String,
    pub message: String,
}

impl From<String> for RunnerError {
    fn from(message: String) -> Self {
        Self {
            kind: "server_error".to_string(),
            message,
        }
    }
}

pub fn run_next() -> Result<RunnerLoopOutcome, RunnerError> {
    let input = match runner_read_input() {
        Ok(input) => input,
        Err(RunnerReadError::InputClosed) => return Ok(RunnerLoopOutcome::InputClosed),
        Err(RunnerReadError::Other(message)) => return Err(message.into()),
    };
    let CodeModeRunnerInput::Start { code, proxy } = input else {
        return Err("runner expected start message".to_string().into());
    };
    reset_execution_jail();
    let runtime = build_runtime()?;
    install_host_bridge(&runtime).map_err(|message| RunnerError {
        kind: "server_error".to_string(),
        message,
    })?;
    run_wrapped(runtime, &code, &proxy)?;
    Ok(RunnerLoopOutcome::Completed)
}

pub fn run_start_without_io(input: CodeModeRunnerInput) -> Result<CodeModeRunnerOutput, String> {
    let CodeModeRunnerInput::Start { code, proxy } = input else {
        return Err("runner expected start message".to_string());
    };
    let runtime = build_runtime().map_err(|err| err.message)?;
    install_no_host_bridge(&runtime)?;
    match run_wrapped_to_result(runtime, &code, &proxy) {
        Ok(result) => Ok(CodeModeRunnerOutput::Done {
            result: CodeModeRunnerResult::from_response_result(result),
            logs: Vec::new(),
        }),
        Err(err) => Ok(CodeModeRunnerOutput::Error {
            kind: err.kind,
            message: err.message,
        }),
    }
}

pub fn emit_error(error: RunnerError) {
    let _ = runner_emit(CodeModeRunnerOutput::Error {
        kind: error.kind,
        message: error.message,
    });
}

pub fn reset_sequence() {
    RUNNER_STATE.with(|state| {
        if let Some(state) = state.borrow_mut().as_mut() {
            state.next_seq = 0;
        }
    });
}

pub fn cleanup_execution_jail(drop_base: bool) {
    crate::runner::jail::cleanup_execution_jail(drop_base);
}

fn build_runtime() -> Result<crate::javy::Runtime, RunnerError> {
    let mut config = crate::javy::Config::default();
    config
        .redirect_stdout_to_stderr(true)
        .memory_limit(MEMORY_LIMIT_BYTES)
        .max_stack_size(CODE_MODE_STACK_SIZE_LIMIT);
    crate::javy::Runtime::new(config).map_err(|err| RunnerError {
        kind: "runtime_error".to_string(),
        message: err.to_string(),
    })
}

fn install_host_bridge(runtime: &crate::javy::Runtime) -> Result<(), String> {
    runtime
        .context()
        .with(|cx| -> crate::javy::quickjs::Result<()> {
            let globals = cx.globals();
            globals.set(
                "__somaEmitToolCall",
                crate::javy::quickjs::Function::new(
                    cx.clone(),
                    crate::javy::quickjs::prelude::MutFn::new(|cx, args| {
                        javy_emit_tool_call(crate::javy::Args::hold(cx, args))
                    }),
                )?,
            )?;
            globals.set(
                "__somaEmitArtifactWrite",
                crate::javy::quickjs::Function::new(
                    cx.clone(),
                    crate::javy::quickjs::prelude::MutFn::new(|cx, args| {
                        javy_emit_artifact_write(crate::javy::Args::hold(cx, args))
                    }),
                )?,
            )?;
            globals.set(
                "__somaEmitSnippetResolve",
                crate::javy::quickjs::Function::new(
                    cx.clone(),
                    crate::javy::quickjs::prelude::MutFn::new(|cx, args| {
                        javy_emit_snippet_resolve(crate::javy::Args::hold(cx, args))
                    }),
                )?,
            )?;
            globals.set(
                "__somaEmitStepBegin",
                crate::javy::quickjs::Function::new(
                    cx.clone(),
                    crate::javy::quickjs::prelude::MutFn::new(|cx, args| {
                        javy_emit_step_begin(crate::javy::Args::hold(cx, args))
                    }),
                )?,
            )?;
            globals.set(
                "__somaEmitStepResult",
                crate::javy::quickjs::Function::new(
                    cx.clone(),
                    crate::javy::quickjs::prelude::MutFn::new(|cx, args| {
                        javy_emit_step_result(crate::javy::Args::hold(cx, args))
                    }),
                )?,
            )?;
            Ok(())
        })
        .map_err(javy_error_message)
}

fn install_no_host_bridge(runtime: &crate::javy::Runtime) -> Result<(), String> {
    runtime
        .context()
        .with(|cx| -> crate::javy::quickjs::Result<()> {
            let globals = cx.globals();
            for name in [
                "__somaEmitToolCall",
                "__somaEmitArtifactWrite",
                "__somaEmitSnippetResolve",
                "__somaEmitStepBegin",
                "__somaEmitStepResult",
            ] {
                globals.set(
                    name,
                    crate::javy::quickjs::Function::new(
                        cx.clone(),
                        crate::javy::quickjs::prelude::MutFn::new(
                            |cx: crate::javy::quickjs::Ctx<'_>| {
                                Err::<u64, _>(javy_type_error(cx, "host bridge unavailable"))
                            },
                        ),
                    )?,
                )?;
            }
            Ok(())
        })
        .map_err(javy_error_message)
}

fn run_wrapped(runtime: crate::javy::Runtime, code: &str, proxy: &str) -> Result<(), RunnerError> {
    let result = run_wrapped_to_result(runtime, code, proxy)?;
    runner_emit(CodeModeRunnerOutput::Done {
        result: CodeModeRunnerResult::from_response_result(result),
        logs: Vec::new(),
    })
    .map_err(RunnerError::from)
}

fn run_wrapped_to_result(
    runtime: crate::javy::Runtime,
    code: &str,
    proxy: &str,
) -> Result<Option<Value>, RunnerError> {
    runtime
        .context()
        .with(|cx| cx.eval::<(), _>(code_mode_runner_script(code, proxy)))
        .map_err(|err| RunnerError {
            kind: "invalid_param".to_string(),
            message: javy_error_message(err),
        })?;
    loop {
        runtime
            .resolve_pending_jobs()
            .map_err(|err| err.to_string())?;
        match javy_main_promise_state(&runtime)? {
            JavyMainPromiseState::Resolved(result) => return Ok(result),
            JavyMainPromiseState::Rejected(message) => return Err(classify_rejection(message)),
            JavyMainPromiseState::Pending => {
                let input = runner_read_input().map_err(RunnerReadError::into_runner_error)?;
                javy_settle_pending_operation(&runtime, &input)?;
            }
        }
    }
}

enum JavyMainPromiseState {
    Pending,
    Resolved(Option<Value>),
    Rejected(String),
}

fn javy_main_promise_state(runtime: &crate::javy::Runtime) -> Result<JavyMainPromiseState, String> {
    runtime
        .context()
        .with(|cx| -> crate::javy::quickjs::Result<JavyMainPromiseState> {
            let promise: crate::javy::quickjs::Promise<'_> =
                cx.globals().get("__somaMainPromise")?;
            match promise.result::<crate::javy::quickjs::Value<'_>>() {
                None => Ok(JavyMainPromiseState::Pending),
                Some(Ok(val)) if val.is_undefined() => Ok(JavyMainPromiseState::Resolved(None)),
                Some(Ok(val)) => match cx.json_stringify(val) {
                    Ok(Some(json_str)) => {
                        let text = json_str.to_string()?;
                        serde_json::from_str(&text)
                            .map(Some)
                            .map(JavyMainPromiseState::Resolved)
                            .or_else(|err| {
                                Ok(JavyMainPromiseState::Rejected(format!(
                                    "Code Mode result must be JSON-serializable: {err}"
                                )))
                            })
                    }
                    Ok(None) => Ok(JavyMainPromiseState::Rejected(
                        "Code Mode result must be JSON-serializable".to_string(),
                    )),
                    Err(err) => Ok(JavyMainPromiseState::Rejected(javy_caught_error_message(
                        &cx, err,
                    ))),
                },
                Some(Err(err)) => Ok(JavyMainPromiseState::Rejected(javy_caught_error_message(
                    &cx, err,
                ))),
            }
        })
        .map_err(javy_error_message)
}

fn javy_settle_pending_operation(
    runtime: &crate::javy::Runtime,
    input: &CodeModeRunnerInput,
) -> Result<(), String> {
    let message = serde_json::to_string(input).map_err(|err| err.to_string())?;
    runtime
        .context()
        .with(|cx| -> crate::javy::quickjs::Result<()> {
            let settle: crate::javy::quickjs::Function<'_> =
                cx.globals().get("__somaSettlePendingOperation")?;
            settle.call::<_, ()>((message,))?;
            Ok(())
        })
        .map_err(javy_error_message)?;
    runtime
        .resolve_pending_jobs()
        .map_err(|err| err.to_string())
}

fn javy_emit_tool_call(args: crate::javy::Args<'_>) -> crate::javy::quickjs::Result<u64> {
    let (cx, args) = args.release();
    let id = required_string_arg(&cx, &args.0, 0, "callTool id must be a non-empty string")?;
    let params = json_arg(&cx, &args.0, 1, "{}")?;
    if !params.is_object() {
        return Err(javy_type_error(cx, "callTool params must be a JSON object"));
    }
    let seq = next_runner_seq(&cx)?;
    runner_emit(CodeModeRunnerOutput::ToolCall { seq, id, params })
        .map_err(|err| javy_type_error(cx, err))?;
    Ok(seq)
}

fn javy_emit_artifact_write(args: crate::javy::Args<'_>) -> crate::javy::quickjs::Result<u64> {
    let (cx, args) = args.release();
    let path = required_string_arg(&cx, &args.0, 0, "writeArtifact path must be a string")?;
    let content = required_string_arg(&cx, &args.0, 1, "writeArtifact content must be a string")?;
    let content_type = optional_string_arg(&cx, &args.0, 2)?;
    let seq = next_runner_seq(&cx)?;
    runner_emit(CodeModeRunnerOutput::ArtifactWrite {
        seq,
        path,
        content,
        content_type,
    })
    .map_err(|err| javy_type_error(cx, err))?;
    Ok(seq)
}

fn javy_emit_snippet_resolve(args: crate::javy::Args<'_>) -> crate::javy::quickjs::Result<u64> {
    let (cx, args) = args.release();
    let name = required_string_arg(&cx, &args.0, 0, "snippet name must be a string")?;
    let input = json_arg(&cx, &args.0, 1, "{}")?;
    let seq = next_runner_seq(&cx)?;
    runner_emit(CodeModeRunnerOutput::SnippetResolve { seq, name, input })
        .map_err(|err| javy_type_error(cx, err))?;
    Ok(seq)
}

fn javy_emit_step_begin(args: crate::javy::Args<'_>) -> crate::javy::quickjs::Result<u64> {
    let (cx, args) = args.release();
    let name = required_string_arg(&cx, &args.0, 0, "codemode.step name must be a string")?;
    let seq = next_runner_seq(&cx)?;
    runner_emit(CodeModeRunnerOutput::StepBegin { seq, name })
        .map_err(|err| javy_type_error(cx, err))?;
    Ok(seq)
}

fn javy_emit_step_result(args: crate::javy::Args<'_>) -> crate::javy::quickjs::Result<u64> {
    let (cx, args) = args.release();
    let seq = args
        .0
        .first()
        .and_then(crate::javy::quickjs::Value::as_number)
        .ok_or_else(|| javy_type_error(cx.clone(), "codemode.step result seq must be a number"))?
        as u64;
    let value = json_arg(&cx, &args.0, 1, "null")?;
    runner_emit(CodeModeRunnerOutput::StepResult { seq, value })
        .map_err(|err| javy_type_error(cx, err))?;
    Ok(seq)
}

fn classify_rejection(message: String) -> RunnerError {
    if let Some(kind) = extract_structured_kind(&message) {
        return RunnerError { kind, message };
    }
    if message.contains("JSON-serializable") {
        return RunnerError {
            kind: "invalid_param".to_string(),
            message,
        };
    }
    RunnerError {
        kind: "server_error".to_string(),
        message,
    }
}

fn extract_structured_kind(message: &str) -> Option<String> {
    let start = message.find('{')?;
    let end = message.rfind('}')?;
    let Value::Object(map) = serde_json::from_str::<Value>(&message[start..=end]).ok()? else {
        return None;
    };
    map.get("kind").and_then(Value::as_str).map(str::to_string)
}

fn next_runner_seq(cx: &crate::javy::quickjs::Ctx<'_>) -> crate::javy::quickjs::Result<u64> {
    RUNNER_STATE
        .with(|state| {
            let mut state = state.borrow_mut();
            let state = state
                .as_mut()
                .ok_or_else(|| "runner state is not initialized".to_string())?;
            let seq = state.next_seq;
            state.next_seq = state.next_seq.saturating_add(1);
            Ok::<_, String>(seq)
        })
        .map_err(|err| javy_type_error(cx.clone(), err))
}

fn runner_emit(output: CodeModeRunnerOutput) -> Result<(), String> {
    use std::io::Write;
    RUNNER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let state = state
            .as_mut()
            .ok_or_else(|| "runner state is not initialized".to_string())?;
        serde_json::to_writer(&mut state.writer, &output).map_err(|err| err.to_string())?;
        state
            .writer
            .write_all(b"\n")
            .map_err(|err| err.to_string())?;
        state.writer.flush().map_err(|err| err.to_string())
    })
}

enum RunnerReadError {
    InputClosed,
    Other(String),
}

impl RunnerReadError {
    fn into_runner_error(self) -> RunnerError {
        match self {
            Self::InputClosed => "runner input closed".to_string().into(),
            Self::Other(message) => message.into(),
        }
    }
}

fn runner_read_input() -> Result<CodeModeRunnerInput, RunnerReadError> {
    use std::io::BufRead;
    RUNNER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let state = state
            .as_mut()
            .ok_or_else(|| RunnerReadError::Other("runner state is not initialized".to_string()))?;
        let mut line = String::new();
        let read = state
            .reader
            .read_line(&mut line)
            .map_err(|err| RunnerReadError::Other(err.to_string()))?;
        if read == 0 {
            return Err(RunnerReadError::InputClosed);
        }
        serde_json::from_str(&line).map_err(|err| RunnerReadError::Other(err.to_string()))
    })
}

fn javy_error_message(error: crate::javy::quickjs::Error) -> String {
    error.to_string()
}

fn javy_caught_error_message(
    cx: &crate::javy::quickjs::Ctx<'_>,
    error: crate::javy::quickjs::Error,
) -> String {
    match crate::javy::quickjs::CaughtError::from_error(cx, error) {
        crate::javy::quickjs::CaughtError::Exception(exception) => {
            exception.message().unwrap_or_else(|| exception.to_string())
        }
        crate::javy::quickjs::CaughtError::Value(value) => {
            crate::javy::val_to_string(cx, value).unwrap_or_else(|err| err.to_string())
        }
        crate::javy::quickjs::CaughtError::Error(error) => error.to_string(),
    }
}
