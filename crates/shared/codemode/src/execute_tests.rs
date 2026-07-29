use super::CodeModeConfig;
use super::execute::execute_inline;
use serial_test::serial;

#[tokio::test]
#[serial(code_mode_soma_home, code_mode_runner_exe_env)]
async fn execute_inline_runs_javy_expression() {
    let response = execute_inline(
        "1 + 2",
        CodeModeConfig {
            enabled: true,
            ..Default::default()
        },
        Default::default(),
    )
    .await
    .unwrap()
    .display_response;
    assert_eq!(response.result.unwrap(), 3);
}
