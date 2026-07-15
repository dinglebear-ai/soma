use serde_json::json;

use super::protocol::{CodeModeRunnerInput, CodeModeRunnerOutput, CodeModeRunnerResult};

#[test]
fn protocol_round_trips_runner_messages() {
    let input = CodeModeRunnerInput::Start {
        code: "async () => 1".to_string(),
        proxy: String::new(),
    };
    assert_eq!(
        serde_json::from_value::<CodeModeRunnerInput>(json!(input)).unwrap(),
        input
    );

    let output = CodeModeRunnerOutput::Done {
        result: CodeModeRunnerResult::Json(json!({"ok": true})),
        logs: vec!["done".to_string()],
    };
    assert_eq!(
        serde_json::from_value::<CodeModeRunnerOutput>(json!(output)).unwrap(),
        output
    );
}
