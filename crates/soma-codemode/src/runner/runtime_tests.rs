use serde_json::json;

use super::runtime::run_start_without_io;
use crate::protocol::CodeModeRunnerInput;
use crate::protocol::CodeModeRunnerResult;

#[test]
fn evaluates_fresh_quickjs_runtime() {
    let output = run_start_without_io(CodeModeRunnerInput::Start {
        code: "async () => ({ answer: 42 })".to_string(),
        proxy: String::new(),
    })
    .unwrap();
    assert_eq!(
        output,
        crate::protocol::CodeModeRunnerOutput::Done {
            result: CodeModeRunnerResult::Json(json!({"answer": 42})),
            logs: Vec::new()
        }
    );
}

#[test]
fn sandbox_has_no_node_or_fetch_globals() {
    let output = run_start_without_io(CodeModeRunnerInput::Start {
        code:
            "async () => ({ fetch: typeof fetch, process: typeof process, require: typeof require })"
                .to_string(),
        proxy: String::new(),
    })
    .unwrap();
    assert_eq!(
        output,
        crate::protocol::CodeModeRunnerOutput::Done {
            result: CodeModeRunnerResult::Json(
                json!({"fetch": "undefined", "process": "undefined", "require": "undefined"})
            ),
            logs: Vec::new()
        }
    );
}
