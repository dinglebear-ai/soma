use super::execute::execute_inline;
use super::CodeModeConfig;

#[tokio::test]
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
