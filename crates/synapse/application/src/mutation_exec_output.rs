use serde_json::{Value, json};
use soma_infra::{HostExecManyOutcome, HostExecTargetStatus, InfraError};
use soma_ops::MutationSendState;

const FANOUT_INLINE_TEXT_BUDGET: usize = 128 * 1024;

pub(crate) fn exec_output(
    exit_code: Option<i64>,
    stdout: String,
    stderr: String,
    timed_out: bool,
    truncated: bool,
) -> Value {
    let mut output = json!({
        "exit_code": exit_code.unwrap_or(-1).clamp(-1, 255),
        "timed_out": timed_out,
        "truncated": truncated,
    });
    if !stdout.is_empty() {
        output["stdout"] = Value::String(stdout);
    }
    if !stderr.is_empty() {
        output["stderr"] = Value::String(stderr);
    }
    output
}

pub(crate) fn many_output(outcome: &HostExecManyOutcome) -> Value {
    let target_count = outcome.results.len().max(1);
    let per_stream_budget = (FANOUT_INLINE_TEXT_BUDGET / target_count / 2).clamp(128, 16 * 1024);
    let results = outcome
        .results
        .iter()
        .map(|result| {
            let target = match &result.working_dir {
                Some(path) => format!("{}:{}", result.host, path.display()),
                None => result.host.to_string(),
            };
            let mut row = json!({
                "target": target,
                "ok": result.status == HostExecTargetStatus::Succeeded,
            });
            if let Some(receipt) = &result.receipt {
                let (stdout, stdout_cut) = truncate_utf8(&receipt.stdout, per_stream_budget);
                let (stderr, stderr_cut) = truncate_utf8(&receipt.stderr, per_stream_budget);
                row["output"] = exec_output(
                    receipt.exit_code.map(i64::from),
                    stdout,
                    stderr,
                    false,
                    receipt.truncated || stdout_cut || stderr_cut,
                );
            } else if result.status == HostExecTargetStatus::TimedOut {
                row["output"] = exec_output(None, String::new(), String::new(), true, false);
            }
            if let Some(error) = &result.error {
                let code = if result.send_state == MutationSendState::Unknown {
                    "mutation.uncertain"
                } else if error.contains("timeout") {
                    "operation.timeout"
                } else if error.contains("cancel") {
                    "operation.cancelled"
                } else {
                    "command.failed"
                };
                row["diagnostic_codes"] = json!([code]);
            }
            row
        })
        .collect::<Vec<_>>();
    json!({
        "results": results,
        "success_count": outcome.succeeded,
        "failure_count": outcome.failed + outcome.timed_out,
        "cancelled_count": outcome.cancelled,
    })
}

fn truncate_utf8(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_owned(), false);
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    (value[..boundary].to_owned(), true)
}

pub(crate) fn failure_code(
    error: &InfraError,
    send_state: MutationSendState,
    timed_out: bool,
) -> &'static str {
    if timed_out
        || matches!(
            error,
            InfraError::Fleet(soma_fleet::FleetError::DeadlineExceeded)
        )
    {
        "operation.timeout"
    } else if matches!(error, InfraError::Fleet(soma_fleet::FleetError::Cancelled)) {
        "operation.cancelled"
    } else if send_state == MutationSendState::Unknown {
        "mutation.uncertain"
    } else {
        "internal.failure"
    }
}

#[cfg(test)]
#[path = "mutation_exec_output_tests.rs"]
mod tests;
