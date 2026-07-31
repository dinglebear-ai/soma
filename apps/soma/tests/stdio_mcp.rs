use std::future::Future;
use std::process::Stdio;

use rmcp::{
    model::CallToolRequestParams,
    service::ServiceExt,
    transport::{ConfigureCommandExt, TokioChildProcess},
};
use serde_json::json;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

// `cargo test --workspace --all-features` runs many process-level integration
// binaries concurrently. Cold Soma child startup can exceed five seconds under
// that workspace load without indicating a protocol failure, so initialize has
// a larger but still finite budget. Once initialized, calls retain a tighter
// bounded response budget.
const STDIO_INITIALIZE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const STDIO_RESPONSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const STDIO_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

async fn bounded_stdio_operation<T, E, F>(
    operation: &str,
    timeout: std::time::Duration,
    future: F,
) -> anyhow::Result<T>
where
    E: std::fmt::Display,
    F: Future<Output = Result<T, E>>,
{
    match tokio::time::timeout(timeout, future).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(anyhow::anyhow!("{operation} failed: {error}")),
        Err(_) => Err(anyhow::anyhow!("{operation} timed out after {timeout:?}")),
    }
}

fn stdio_temp_context() -> anyhow::Result<(tempfile::TempDir, std::path::PathBuf)> {
    let temp_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
    std::fs::create_dir_all(&temp_root)?;
    let temp = tempfile::Builder::new()
        .prefix("soma-stdio-")
        .tempdir_in(temp_root)?;
    let work_dir = temp.path().join("work");
    std::fs::create_dir(&work_dir)?;
    Ok((temp, work_dir))
}

async fn stdio_client() -> anyhow::Result<(
    rmcp::service::RunningService<rmcp::RoleClient, ()>,
    Option<tokio::process::ChildStderr>,
    tempfile::TempDir,
)> {
    let binary = env!("CARGO_BIN_EXE_soma");
    let (temp, work_dir) = stdio_temp_context()?;
    let (transport, stderr) = TokioChildProcess::builder(Command::new(binary).configure(|cmd| {
        cmd.arg("mcp")
            .current_dir(&work_dir)
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

async fn raw_stdio_child()
-> anyhow::Result<(Child, ChildStdin, BufReader<ChildStdout>, tempfile::TempDir)> {
    let binary = env!("CARGO_BIN_EXE_soma");
    let (temp, work_dir) = stdio_temp_context()?;
    let mut child = Command::new(binary)
        .arg("mcp")
        .current_dir(&work_dir)
        .env("HOME", temp.path())
        .env("SOMA_HOME", temp.path())
        .env("SOMA_API_URL", "")
        .env("SOMA_API_KEY", "")
        .env("RUST_LOG", "warn")
        .env("SOMA_MCP_TOKEN", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdin = child.stdin.take().expect("stdio child should expose stdin");
    let stdout = child
        .stdout
        .take()
        .expect("stdio child should expose stdout");
    Ok((child, stdin, BufReader::new(stdout), temp))
}

async fn write_json_line(stdin: &mut ChildStdin, value: serde_json::Value) -> anyhow::Result<()> {
    let mut line = serde_json::to_vec(&value)?;
    line.push(b'\n');
    stdin.write_all(&line).await?;
    stdin.flush().await?;
    Ok(())
}

async fn read_json_rpc_response(
    stdout: &mut BufReader<ChildStdout>,
    id: u64,
    timeout: std::time::Duration,
) -> anyhow::Result<serde_json::Value> {
    let mut line = String::new();
    tokio::time::timeout(timeout, async {
        loop {
            line.clear();
            let bytes = stdout.read_line(&mut line).await?;
            anyhow::ensure!(
                bytes > 0,
                "stdio child closed stdout before response id {id}"
            );
            let value: serde_json::Value = serde_json::from_str(line.trim_end())?;
            if value["id"].as_u64() == Some(id) {
                return Ok(value);
            }
        }
    })
    .await
    .map_err(|_| {
        anyhow::anyhow!("timed out after {timeout:?} waiting for JSON-RPC response id {id}")
    })?
}

#[tokio::test]
async fn stdio_child_process_lists_tools_and_calls_actions() -> anyhow::Result<()> {
    let (service, stderr, _temp) = bounded_stdio_operation(
        "stdio client initialize",
        STDIO_INITIALIZE_TIMEOUT,
        stdio_client(),
    )
    .await?;

    let tools = bounded_stdio_operation(
        "stdio tools/list",
        STDIO_RESPONSE_TIMEOUT,
        service.list_tools(Default::default()),
    )
    .await?;
    let names: Vec<&str> = tools.tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert_eq!(names, vec!["soma"]);
    assert_eq!(
        tools.tools[0]
            .output_schema
            .as_ref()
            .expect("soma should advertise structured output")["type"],
        "object"
    );

    let status = bounded_stdio_operation(
        "stdio status tool call",
        STDIO_RESPONSE_TIMEOUT,
        service.call_tool(
            CallToolRequestParams::new("soma")
                .with_arguments(json!({"action": "status"}).as_object().unwrap().clone()),
        ),
    )
    .await?;
    let status = structured_result_json(&status);
    assert_eq!(
        status["output"]["status"], "ok",
        "status payload was {status}"
    );

    let echo = bounded_stdio_operation(
        "stdio echo tool call",
        STDIO_RESPONSE_TIMEOUT,
        service.call_tool(
            CallToolRequestParams::new("soma").with_arguments(
                json!({"action": "echo", "message": "stdio works"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        ),
    )
    .await?;
    let echo = structured_result_json(&echo);
    assert_eq!(echo["output"]["echo"], "stdio works");

    bounded_stdio_operation(
        "stdio client cancellation",
        STDIO_SHUTDOWN_TIMEOUT,
        service.cancel(),
    )
    .await?;

    if let Some(mut stderr) = stderr {
        let mut logs = String::new();
        bounded_stdio_operation(
            "stdio child stderr closure",
            STDIO_SHUTDOWN_TIMEOUT,
            stderr.read_to_string(&mut logs),
        )
        .await?;
        assert!(
            !logs.contains("MCP server listening") && !logs.contains("HTTP server"),
            "stdio mode must not start network services; stderr was: {logs}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn raw_stdio_json_rpc_preserves_structured_output_fields() -> anyhow::Result<()> {
    let (mut child, mut stdin, mut stdout, _temp) = raw_stdio_child().await?;

    write_json_line(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {
                    "name": "soma-raw-stdio-test",
                    "version": "0.0.0"
                }
            }
        }),
    )
    .await?;
    let initialize = read_json_rpc_response(&mut stdout, 1, STDIO_INITIALIZE_TIMEOUT).await?;
    assert_eq!(initialize["jsonrpc"], "2.0");
    assert!(initialize["result"]["protocolVersion"].is_string());

    write_json_line(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }),
    )
    .await?;

    write_json_line(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }),
    )
    .await?;
    let tools = read_json_rpc_response(&mut stdout, 2, STDIO_RESPONSE_TIMEOUT).await?;
    let tool = &tools["result"]["tools"][0];
    assert_eq!(tool["name"], "soma");
    assert_eq!(tool["outputSchema"]["type"], "object");
    assert_eq!(
        tool["outputSchema"]["x-soma-action-discriminator"],
        "_soma_action"
    );
    assert!(
        tool["outputSchema"]["oneOf"]
            .as_array()
            .expect("outputSchema.oneOf should be serialized")
            .iter()
            .any(|variant| variant["properties"]["_soma_action"]["const"] == "echo")
    );

    write_json_line(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "soma",
                "arguments": {
                    "action": "echo",
                    "message": "raw stdio works"
                }
            }
        }),
    )
    .await?;
    let echo = read_json_rpc_response(&mut stdout, 3, STDIO_RESPONSE_TIMEOUT).await?;
    let echo_result = &echo["result"];
    assert_eq!(echo_result["structuredContent"]["_soma_action"], "echo");
    assert_eq!(
        echo_result["structuredContent"]["output"]["echo"],
        "raw stdio works"
    );
    let echo_text: serde_json::Value =
        serde_json::from_str(echo_result["content"][0]["text"].as_str().unwrap())?;
    assert_eq!(echo_text, echo_result["structuredContent"]);

    write_json_line(
        &mut stdin,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "soma",
                "arguments": {
                    "action": "echo"
                }
            }
        }),
    )
    .await?;
    let error = read_json_rpc_response(&mut stdout, 4, STDIO_RESPONSE_TIMEOUT).await?;
    let error_result = &error["result"];
    assert_eq!(error_result["isError"], true);
    assert_eq!(error_result["structuredContent"]["kind"], "mcp_tool_error");
    assert_eq!(
        error_result["structuredContent"]["code"],
        "input_schema_failed"
    );
    let error_text: serde_json::Value =
        serde_json::from_str(error_result["content"][0]["text"].as_str().unwrap())?;
    assert_eq!(error_text, error_result["structuredContent"]);

    drop(stdin);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await;
    Ok(())
}
