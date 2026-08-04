use std::process::Stdio;
use std::time::Duration;

use rmcp::{
    model::CallToolRequestParams,
    service::ServiceExt,
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use serde_json::json;
use tokio::process::Command;

const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(30);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);

#[tokio::test]
async fn stdio_binary_discovers_and_calls_canonical_runtime() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let work = temp.path().join("work");
    std::fs::create_dir(&work)?;
    let binary = env!("CARGO_BIN_EXE_synapse");
    let (transport, _stderr) =
        TokioChildProcess::builder(Command::new(binary).configure(|command| {
            command
                .arg("mcp")
                .current_dir(&work)
                .env("HOME", temp.path())
                .env_remove("SYNAPSE_CONFIG")
                .env("RUST_LOG", "warn");
        }))
        .stderr(Stdio::piped())
        .spawn()?;
    let service = tokio::time::timeout(INITIALIZE_TIMEOUT, ().serve(transport))
        .await
        .map_err(|_| anyhow::anyhow!("stdio MCP initialization timed out"))??;

    let tools = tokio::time::timeout(RESPONSE_TIMEOUT, service.list_tools(Default::default()))
        .await
        .map_err(|_| anyhow::anyhow!("stdio MCP tools/list timed out"))??;
    let names = tools
        .tools
        .iter()
        .map(|tool| tool.name.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["synapse", "flux", "scout"]);

    let result = tokio::time::timeout(
        RESPONSE_TIMEOUT,
        service.call_tool(
            CallToolRequestParams::new("synapse").with_arguments(
                json!({"operation":"product.help","parameters":{}})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        ),
    )
    .await
    .map_err(|_| anyhow::anyhow!("stdio MCP tool call timed out"))??;
    assert_eq!(result.is_error, Some(false));
    let output = result
        .structured_content
        .expect("canonical tool result should be structured JSON");
    assert!(
        output["operations"]
            .as_array()
            .is_some_and(|operations| operations.len() == 59),
        "unexpected product.help output: {output}"
    );

    tokio::time::timeout(RESPONSE_TIMEOUT, service.cancel())
        .await
        .map_err(|_| anyhow::anyhow!("stdio MCP shutdown timed out"))??;
    Ok(())
}
