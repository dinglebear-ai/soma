use super::protocol::{CodeModeRunnerInput, CodeModeRunnerOutput};
use super::runner_io::{decode_runner_input, decode_runner_output, encode_runner_input};

#[test]
fn runner_io_round_trips_newline_framed_json() {
    let input = CodeModeRunnerInput::Start {
        code: "async () => 1".to_string(),
        proxy: String::new(),
    };
    let line = encode_runner_input(&input).unwrap();
    assert_eq!(decode_runner_input(&line).unwrap(), input);
    let output = r#"{"type":"error","kind":"x","message":"y"}"#;
    assert!(matches!(
        decode_runner_output(output).unwrap(),
        CodeModeRunnerOutput::Error { .. }
    ));
}
