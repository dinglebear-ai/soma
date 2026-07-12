use std::process::Stdio;

use rmcp::{
    model::CallToolRequestParams,
    service::ServiceExt,
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use serde_json::json;
use tokio::{io::AsyncReadExt, process::Command};

async fn stdio_client() -> anyhow::Result<(
    rmcp::service::RunningService<rmcp::RoleClient, ()>,
    Option<tokio::process::ChildStderr>,
    tempfile::TempDir,
)> {
    let binary = env!("CARGO_BIN_EXE_soma");
    let temp = tempfile::tempdir()?;
    let (transport, stderr) = TokioChildProcess::builder(Command::new(binary).configure(|cmd| {
        cmd.arg("mcp")
            .current_dir(temp.path())
            .env("HOME", temp.path())
            .env("SOMA_HOME", temp.path())
            .env("SOMA_API_URL", "")
            .env("SOMA_API_KEY", "")
            .env("RUST_LOG", "warn")
            .env("SOMA_MCP_TOKEN", "");
    }))
    .stderr(Stdio::piped())
    .spawn()?;
    let service = ().serve(transport).await?;
    Ok((service, stderr, temp))
}

fn structured_result_json(result: &rmcp::model::CallToolResult) -> serde_json::Value {
    if let Some(value) = result.structured_content.clone() {
        return value;
    }
    let value = serde_json::to_value(result).expect("tool result should serialize");
    let text = value["content"][0]["text"]
        .as_str()
        .expect("tool result should contain text content");
    serde_json::from_str(text).expect("tool text content should be JSON")
}

#[tokio::test]
async fn stdio_child_process_lists_tools_and_calls_actions() {
    let (service, stderr, _temp) = stdio_client().await.unwrap();

    let tools = service.list_tools(Default::default()).await.unwrap();
    let names: Vec<&str> = tools.tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert_eq!(names, vec!["soma"]);
    assert_eq!(
        tools.tools[0]
            .output_schema
            .as_ref()
            .expect("soma should advertise structured output")["type"],
        "object"
    );

    let status = service
        .call_tool(
            CallToolRequestParams::new("soma")
                .with_arguments(json!({"action": "status"}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let status = structured_result_json(&status);
    assert_eq!(status["status"], "ok", "status payload was {status}");

    let echo = service
        .call_tool(
            CallToolRequestParams::new("soma").with_arguments(
                json!({"action": "echo", "message": "stdio works"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    let echo = structured_result_json(&echo);
    assert_eq!(echo["echo"], "stdio works");

    service.cancel().await.unwrap();

    if let Some(mut stderr) = stderr {
        let mut logs = String::new();
        match tokio::time::timeout(
            std::time::Duration::from_secs(1),
            stderr.read_to_string(&mut logs),
        )
        .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => panic!("failed to read stdio child stderr: {error}"),
            Err(_) => panic!("stdio child stderr did not close after cancellation"),
        }
        assert!(
            !logs.contains("MCP server listening") && !logs.contains("HTTP server"),
            "stdio mode must not start network services; stderr was: {logs}"
        );
    }
}
