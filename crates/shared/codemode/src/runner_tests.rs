use super::protocol::{CodeModeRunnerInput, CodeModeRunnerOutput, CodeModeRunnerResult};
use super::runner::run_code_mode_runner_once;

#[test]
fn runner_once_handles_start_message() {
    let output = run_code_mode_runner_once(CodeModeRunnerInput::Start {
        code: "async () => 7".to_string(),
        proxy: String::new(),
    })
    .unwrap();
    assert!(matches!(
        output,
        CodeModeRunnerOutput::Done {
            result: CodeModeRunnerResult::Json(_),
            ..
        }
    ));
}
