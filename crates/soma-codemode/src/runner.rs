pub mod jail;
pub mod js_args;
pub mod limits;
pub mod runtime;
pub mod steps;

#[cfg(test)]
mod jail_tests;
#[cfg(test)]
mod js_args_tests;
#[cfg(test)]
mod limits_tests;
#[cfg(test)]
mod runtime_tests;
#[cfg(test)]
mod steps_tests;

use std::io::{BufReader, BufWriter};

use crate::protocol::{
    CodeModeRunnerInput, CodeModeRunnerOutput, CodeModeRunnerResult, CodeModeRunnerState,
    RUNNER_STATE,
};

pub fn run_code_mode_runner_stdio_blocking() -> Result<(), String> {
    #[cfg(all(unix, target_os = "linux"))]
    {
        let _ = nix::sys::prctl::set_dumpable(false);
    }
    RUNNER_STATE.with(|state| {
        *state.borrow_mut() = Some(CodeModeRunnerState {
            reader: BufReader::new(std::io::stdin()),
            writer: BufWriter::new(std::io::stdout()),
            next_seq: 0,
        });
    });
    loop {
        match runtime::run_next() {
            Ok(runtime::RunnerLoopOutcome::Completed) => {
                runtime::cleanup_execution_jail(false);
                runtime::reset_sequence();
            }
            Ok(runtime::RunnerLoopOutcome::InputClosed) => {
                runtime::cleanup_execution_jail(true);
                return Ok(());
            }
            Err(error) => {
                runtime::emit_error(error);
                runtime::cleanup_execution_jail(false);
                runtime::reset_sequence();
            }
        }
    }
}

pub fn run_code_mode_runner_once(
    input: CodeModeRunnerInput,
) -> Result<CodeModeRunnerOutput, String> {
    runtime::run_start_without_io(input)
}

pub fn result_from_value(value: Option<serde_json::Value>) -> CodeModeRunnerResult {
    CodeModeRunnerResult::from_response_result(value)
}
