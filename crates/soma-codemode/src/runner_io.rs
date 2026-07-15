use tokio::io::AsyncWriteExt;

use crate::protocol::{CodeModeRunnerInput, CodeModeRunnerOutput};
use crate::runner::limits::MAX_STDIO_LINE_BYTES;
use crate::ToolError;

pub fn encode_runner_input(input: &CodeModeRunnerInput) -> Result<String, ToolError> {
    encode_json_line(input, "input")
}

pub fn encode_runner_output(output: &CodeModeRunnerOutput) -> String {
    serde_json::to_string(output).unwrap_or_else(|_| {
        r#"{"type":"error","kind":"internal_error","message":"failed to encode output"}"#
            .to_string()
    })
}

pub fn decode_runner_input(line: &str) -> Result<CodeModeRunnerInput, ToolError> {
    if line.len() > MAX_STDIO_LINE_BYTES {
        return Err(ToolError::Sdk {
            sdk_kind: "invalid_param".to_string(),
            message: "runner input line exceeded limit".to_string(),
        });
    }
    serde_json::from_str(line).map_err(|err| ToolError::Sdk {
        sdk_kind: "invalid_param".to_string(),
        message: format!("invalid runner input JSON: {err}"),
    })
}

pub fn decode_runner_output(line: &str) -> Result<CodeModeRunnerOutput, ToolError> {
    if line.len() > MAX_STDIO_LINE_BYTES {
        return Err(ToolError::Sdk {
            sdk_kind: "invalid_param".to_string(),
            message: "runner output line exceeded limit".to_string(),
        });
    }
    serde_json::from_str(line).map_err(|err| ToolError::Sdk {
        sdk_kind: "invalid_param".to_string(),
        message: format!("invalid runner output JSON: {err}"),
    })
}

pub async fn write_runner_input<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    input: &CodeModeRunnerInput,
) -> Result<(), ToolError> {
    let mut line = encode_runner_input(input)?;
    line.push('\n');
    writer.write_all(line.as_bytes()).await.map_err(|err| {
        ToolError::internal_message(format!("failed to write runner input: {err}"))
    })?;
    writer
        .flush()
        .await
        .map_err(|err| ToolError::internal_message(format!("failed to flush runner input: {err}")))
}

pub async fn terminate_code_mode_runner(child: &mut tokio::process::Child, child_pid: Option<u32>) {
    #[cfg(unix)]
    if let Some(pid) = child_pid {
        use nix::sys::signal::Signal;
        use nix::unistd::Pid;
        let _ = nix::sys::signal::killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

fn encode_json_line<T: serde::Serialize>(value: &T, label: &str) -> Result<String, ToolError> {
    let encoded = serde_json::to_string(value).map_err(|err| ToolError::Sdk {
        sdk_kind: "internal_error".to_string(),
        message: format!("failed to encode runner {label}: {err}"),
    })?;
    if encoded.len() > MAX_STDIO_LINE_BYTES {
        return Err(ToolError::Sdk {
            sdk_kind: "invalid_param".to_string(),
            message: format!("runner {label} line exceeded limit"),
        });
    }
    Ok(encoded)
}
