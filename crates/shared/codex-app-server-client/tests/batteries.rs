use std::time::Duration;

use codex_app_server_client::protocol::{
    AgentMessageDeltaNotification, ClientInfo, ConfigReadParams, CurrentTimeReadParams,
    InitializeParams, ServerNotification, ServerRequest, ThreadStartParams, TurnStartParams,
    UserInput,
};
use codex_app_server_client::{
    AllowAllApprovalHandler, ApprovalHandler, CodexAppServerClient, CodexDaemon, CodexSession,
    CompatibilityReport, DenyAllApprovalHandler, Error, EventCollector, ReadOnlyApprovalHandler,
    ServerRequestReply, SessionOptions, SurfaceSummary, CODEX_SCHEMA_VERSION,
};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

#[test]
fn builders_cover_the_common_no_json_path() {
    let info = ClientInfo::new("demo-client", "0.2.0").with_title("Demo Client");
    assert_eq!(info.name, "demo-client");
    assert_eq!(info.title.as_deref(), Some("Demo Client"));

    let init = InitializeParams::for_client("demo-client", "0.2.0");
    assert_eq!(init.client_info.name, "demo-client");
    assert!(init.capabilities.is_none());

    let thread = ThreadStartParams::new()
        .model("gpt-5")
        .cwd("/workspace")
        .ephemeral(true);
    assert_eq!(thread.model.as_deref(), Some("gpt-5"));
    assert_eq!(thread.cwd.as_deref(), Some("/workspace"));
    assert_eq!(thread.ephemeral, Some(true));

    let turn = TurnStartParams::text("thread-1", "hello");
    assert_eq!(turn.thread_id, "thread-1");
    assert_eq!(turn.input.len(), 1);
    assert!(matches!(&turn.input[0], UserInput::Text { text, .. } if text == "hello"));

    let config = ConfigReadParams::for_cwd("/workspace").include_layers(true);
    assert_eq!(config.cwd.as_deref(), Some("/workspace"));
    assert_eq!(config.include_layers, Some(true));
}

#[test]
fn compatibility_report_summarizes_schema_and_installed_version() {
    let report = CompatibilityReport::from_installed_version(Some(CODEX_SCHEMA_VERSION.to_owned()));
    assert_eq!(report.schema_codex_version, CODEX_SCHEMA_VERSION);
    assert_eq!(
        report.surface,
        SurfaceSummary {
            client_requests: 122,
            server_requests: 11,
            server_notifications: 68,
            client_notifications: 1,
        }
    );
    assert_eq!(
        report.installed_codex_version.as_deref(),
        Some(CODEX_SCHEMA_VERSION)
    );
    assert!(report.schema_matches_installed());

    let stale = CompatibilityReport::from_installed_version(Some("codex-cli 999.0.0".to_owned()));
    assert!(!stale.schema_matches_installed());
}

#[test]
fn event_collector_aggregates_text_diff_completion_and_errors() {
    let mut collector = EventCollector::for_turn("thread-1", "turn-1");

    collector.observe_notification(&ServerNotification::ItemAgentMessageDelta(
        AgentMessageDeltaNotification {
            delta: "hello ".to_owned(),
            item_id: "item-1".to_owned(),
            thread_id: "thread-1".to_owned(),
            turn_id: "turn-1".to_owned(),
        },
    ));
    collector.observe_notification(&ServerNotification::ItemAgentMessageDelta(
        AgentMessageDeltaNotification {
            delta: "world".to_owned(),
            item_id: "item-1".to_owned(),
            thread_id: "thread-1".to_owned(),
            turn_id: "turn-1".to_owned(),
        },
    ));

    let diff = serde_json::json!({
        "method": "turn/diff/updated",
        "params": {
            "threadId": "thread-1",
            "turnId": "turn-1",
            "diff": "diff --git a/file b/file"
        }
    });
    let diff = serde_json::from_value::<ServerNotification>(diff).unwrap();
    collector.observe_notification(&diff);

    let completed = serde_json::json!({
        "method": "turn/completed",
        "params": {
            "threadId": "thread-1",
            "turn": {
                "id": "turn-1",
                "items": [],
                "status": "completed"
            }
        }
    });
    let completed = serde_json::from_value::<ServerNotification>(completed).unwrap();
    collector.observe_notification(&completed);

    assert_eq!(collector.agent_message(), "hello world");
    assert_eq!(collector.latest_diff(), Some("diff --git a/file b/file"));
    assert!(collector.is_complete());
    assert_eq!(collector.errors().len(), 0);
}

#[test]
fn deny_all_approval_handler_replies_with_structured_error() {
    let request = ServerRequest::CurrentTimeRead {
        id: codex_app_server_client::protocol::RequestId::Int64(7),
        params: CurrentTimeReadParams {
            thread_id: "thread-1".to_owned(),
        },
    };
    let handler = DenyAllApprovalHandler::default();

    let reply = handler.handle(&request);
    assert!(matches!(
        reply,
        ServerRequestReply::Error { code: -32000, .. }
    ));
}

#[test]
fn daemon_helper_builds_real_app_server_listen_args() {
    let daemon = CodexDaemon::new("/tmp/codex.sock").with_config("model", "\"gpt-5\"");
    assert_eq!(
        daemon.app_server_args(),
        vec![
            "app-server",
            "--listen",
            "unix:///tmp/codex.sock",
            "-c",
            "model=\"gpt-5\""
        ]
    );
    assert_eq!(
        daemon.start_args(),
        vec!["app-server", "daemon", "-c", "model=\"gpt-5\"", "start"]
    );
}

#[tokio::test]
async fn session_connect_streams_handshakes_before_returning() {
    let (client_io, server_io) = tokio::io::duplex(4096);
    let (client_read, client_write) = tokio::io::split(client_io);
    let (server_read, mut server_write) = tokio::io::split(server_io);

    let server = tokio::spawn(async move {
        let mut reader = BufReader::new(server_read);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let init: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(init["method"], "initialize");
        let id = init["id"].clone();
        let response = serde_json::json!({
            "id": id,
            "result": {
                "codexHome": "/tmp/codex-home",
                "platformFamily": "unix",
                "platformOs": "linux",
                "userAgent": "fake-codex"
            }
        });
        server_write
            .write_all(format!("{response}\n").as_bytes())
            .await
            .unwrap();

        line.clear();
        reader.read_line(&mut line).await.unwrap();
        let initialized: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(initialized["method"], "initialized");
    });

    let options =
        SessionOptions::new("test-client", "0.1.0").with_call_timeout(Duration::from_secs(5));
    let session = CodexSession::connect_streams(BufReader::new(client_read), client_write, options)
        .await
        .unwrap();

    assert_eq!(session.initialize_response().platform_os, "linux");
    server.await.unwrap();
}

#[tokio::test]
async fn raw_method_calls_bridge_direct_json_rpc_requests() {
    let (client_io, server_io) = tokio::io::duplex(4096);
    let (client_read, client_write) = tokio::io::split(client_io);
    let (server_read, mut server_write) = tokio::io::split(server_io);

    let server = tokio::spawn(async move {
        let mut reader = BufReader::new(server_read);
        let request = read_json_line(&mut reader).await;
        assert_eq!(request["method"], "thread/start");
        assert_eq!(request["params"]["model"], "gpt-5");
        assert_eq!(request["params"]["cwd"], "/workspace");
        respond_json(
            &mut server_write,
            request["id"].clone(),
            serde_json::json!({
                "thread": {
                    "id": "thread-raw",
                    "title": "raw bridge",
                    "model": "gpt-5"
                }
            }),
        )
        .await;
    });

    let (client, _events) =
        CodexAppServerClient::connect_streams(BufReader::new(client_read), client_write);
    let result = client
        .with_call_timeout(Duration::from_secs(5))
        .call_raw_method(
            "thread/start",
            serde_json::json!({
                "model": "gpt-5",
                "cwd": "/workspace"
            }),
        )
        .await
        .unwrap();

    assert_eq!(result["thread"]["id"], "thread-raw");
    server.await.unwrap();
}

#[tokio::test]
async fn text_turn_helpers_start_send_and_collect_output() {
    let (client_io, server_io) = tokio::io::duplex(8192);
    let (client_read, client_write) = tokio::io::split(client_io);
    let (server_read, mut server_write) = tokio::io::split(server_io);

    let server = tokio::spawn(async move {
        let mut reader = BufReader::new(server_read);
        respond_to_initialize(&mut reader, &mut server_write).await;

        let thread_start = read_json_line(&mut reader).await;
        assert_eq!(thread_start["method"], "thread/start");
        assert_eq!(thread_start["params"]["model"], "gpt-5");
        respond_json(
            &mut server_write,
            thread_start["id"].clone(),
            fake_thread_start_response("thread-1", "gpt-5"),
        )
        .await;

        let turn_start = read_json_line(&mut reader).await;
        assert_eq!(turn_start["method"], "turn/start");
        assert_eq!(turn_start["params"]["threadId"], "thread-1");
        assert_eq!(turn_start["params"]["input"][0]["text"], "hello");
        respond_json(
            &mut server_write,
            turn_start["id"].clone(),
            serde_json::json!({
                "turn": {
                    "id": "turn-1",
                    "items": [],
                    "itemsView": "notLoaded",
                    "status": "inProgress"
                }
            }),
        )
        .await;

        write_notification(
            &mut server_write,
            serde_json::json!({
                "method": "item/agentMessage/delta",
                "params": {
                    "threadId": "thread-1",
                    "turnId": "turn-1",
                    "itemId": "item-1",
                    "delta": "hello from mock"
                }
            }),
        )
        .await;
        write_notification(
            &mut server_write,
            serde_json::json!({
                "method": "turn/completed",
                "params": {
                    "threadId": "thread-1",
                    "turn": {
                        "id": "turn-1",
                        "items": [],
                        "itemsView": "notLoaded",
                        "status": "completed"
                    }
                }
            }),
        )
        .await;
    });

    let mut session = CodexSession::connect_streams(
        BufReader::new(client_read),
        client_write,
        SessionOptions::new("test-client", "0.1.0").with_call_timeout(Duration::from_secs(5)),
    )
    .await
    .unwrap();
    let result = session
        .run_text_turn_with_model_and_handler("gpt-5", "hello", &DenyAllApprovalHandler::default())
        .await
        .unwrap();

    assert_eq!(result.thread.thread.id, "thread-1");
    assert_eq!(result.turn.turn.id, "turn-1");
    assert_eq!(result.agent_message(), "hello from mock");
    assert!(result.errors().is_empty());
    server.await.unwrap();
}

#[tokio::test]
async fn event_waiters_collect_named_turns() {
    let (client_io, server_io) = tokio::io::duplex(8192);
    let (client_read, client_write) = tokio::io::split(client_io);
    let (server_read, mut server_write) = tokio::io::split(server_io);

    let server = tokio::spawn(async move {
        let mut reader = BufReader::new(server_read);
        respond_to_initialize(&mut reader, &mut server_write).await;

        let thread_start = read_json_line(&mut reader).await;
        respond_json(
            &mut server_write,
            thread_start["id"].clone(),
            fake_thread_start_response("thread-2", "gpt-5-mini"),
        )
        .await;

        let turn_start = read_json_line(&mut reader).await;
        respond_json(
            &mut server_write,
            turn_start["id"].clone(),
            serde_json::json!({
                "turn": {
                    "id": "turn-2",
                    "items": [],
                    "itemsView": "notLoaded",
                    "status": "inProgress"
                }
            }),
        )
        .await;

        write_notification(
            &mut server_write,
            serde_json::json!({
                "method": "item/agentMessage/delta",
                "params": {
                    "threadId": "thread-2",
                    "turnId": "turn-2",
                    "itemId": "item-1",
                    "delta": "collected text"
                }
            }),
        )
        .await;
        write_notification(
            &mut server_write,
            serde_json::json!({
                "method": "turn/completed",
                "params": {
                    "threadId": "thread-2",
                    "turn": {
                        "id": "turn-2",
                        "items": [],
                        "itemsView": "notLoaded",
                        "status": "completed"
                    }
                }
            }),
        )
        .await;
    });

    let mut session = CodexSession::connect_streams(
        BufReader::new(client_read),
        client_write,
        SessionOptions::new("test-client", "0.1.0").with_call_timeout(Duration::from_secs(5)),
    )
    .await
    .unwrap();
    let thread = session.start_thread_with_model("gpt-5-mini").await.unwrap();
    let turn = session
        .send_text_turn(&thread.thread.id, "collect this")
        .await
        .unwrap();
    let text = session
        .collect_agent_message(
            &thread.thread.id,
            &turn.turn.id,
            &DenyAllApprovalHandler::default(),
        )
        .await
        .unwrap();

    assert_eq!(text, "collected text");
    server.await.unwrap();
}

#[tokio::test]
async fn one_call_run_text_turn_uses_default_thread_and_collects_diff() {
    let (client_io, server_io) = tokio::io::duplex(8192);
    let (client_read, client_write) = tokio::io::split(client_io);
    let (server_read, mut server_write) = tokio::io::split(server_io);

    let server = tokio::spawn(async move {
        let mut reader = BufReader::new(server_read);
        respond_to_initialize(&mut reader, &mut server_write).await;

        let thread_start = read_json_line(&mut reader).await;
        assert_eq!(thread_start["method"], "thread/start");
        assert!(thread_start["params"].get("model").is_none());
        respond_json(
            &mut server_write,
            thread_start["id"].clone(),
            fake_thread_start_response("thread-3", "default-model"),
        )
        .await;

        let turn_start = read_json_line(&mut reader).await;
        assert_eq!(turn_start["method"], "turn/start");
        assert_eq!(turn_start["params"]["threadId"], "thread-3");
        assert_eq!(turn_start["params"]["input"][0]["text"], "simple prompt");
        respond_json(
            &mut server_write,
            turn_start["id"].clone(),
            serde_json::json!({
                "turn": {
                    "id": "turn-3",
                    "items": [],
                    "itemsView": "notLoaded",
                    "status": "inProgress"
                }
            }),
        )
        .await;

        write_notification(
            &mut server_write,
            serde_json::json!({
                "method": "turn/diff/updated",
                "params": {
                    "threadId": "thread-3",
                    "turnId": "turn-3",
                    "diff": "diff --git a/a b/a"
                }
            }),
        )
        .await;
        write_notification(
            &mut server_write,
            serde_json::json!({
                "method": "item/agentMessage/delta",
                "params": {
                    "threadId": "thread-3",
                    "turnId": "turn-3",
                    "itemId": "item-1",
                    "delta": "default helper"
                }
            }),
        )
        .await;
        write_notification(
            &mut server_write,
            serde_json::json!({
                "method": "turn/completed",
                "params": {
                    "threadId": "thread-3",
                    "turn": {
                        "id": "turn-3",
                        "items": [],
                        "itemsView": "notLoaded",
                        "status": "completed"
                    }
                }
            }),
        )
        .await;
    });

    let mut session = CodexSession::connect_streams(
        BufReader::new(client_read),
        client_write,
        SessionOptions::new("test-client", "0.1.0").with_call_timeout(Duration::from_secs(5)),
    )
    .await
    .unwrap();
    let result = session.run_text_turn("simple prompt").await.unwrap();

    assert_eq!(result.agent_message(), "default helper");
    assert_eq!(result.latest_diff(), Some("diff --git a/a b/a"));
    server.await.unwrap();
}

#[tokio::test]
async fn text_turn_errors_if_transport_closes_before_completion() {
    let (client_io, server_io) = tokio::io::duplex(8192);
    let (client_read, client_write) = tokio::io::split(client_io);
    let (server_read, mut server_write) = tokio::io::split(server_io);

    let server = tokio::spawn(async move {
        let mut reader = BufReader::new(server_read);
        respond_to_initialize(&mut reader, &mut server_write).await;

        let thread_start = read_json_line(&mut reader).await;
        respond_json(
            &mut server_write,
            thread_start["id"].clone(),
            fake_thread_start_response("thread-closed", "default-model"),
        )
        .await;

        let turn_start = read_json_line(&mut reader).await;
        respond_json(
            &mut server_write,
            turn_start["id"].clone(),
            serde_json::json!({
                "turn": {
                    "id": "turn-closed",
                    "items": [],
                    "itemsView": "notLoaded",
                    "status": "inProgress"
                }
            }),
        )
        .await;
        // Drop the writer without sending turn/completed.
    });

    let mut session = CodexSession::connect_streams(
        BufReader::new(client_read),
        client_write,
        SessionOptions::new("test-client", "0.1.0").with_call_timeout(Duration::from_secs(5)),
    )
    .await
    .unwrap();
    let err = session
        .run_text_turn("never completes")
        .await
        .expect_err("early transport close must not look like a successful turn");

    assert!(matches!(err, Error::TransportClosed));
    server.await.unwrap();
}

#[test]
fn approval_presets_serialize_expected_schema_replies() {
    let command_request = serde_json::from_value::<ServerRequest>(serde_json::json!({
        "method": "item/commandExecution/requestApproval",
        "id": 1,
        "params": {
            "threadId": "thread-1",
            "turnId": "turn-1",
            "itemId": "item-1",
            "command": "cargo test",
            "cwd": "/workspace",
            "source": "agent",
            "startedAtMs": 1
        }
    }))
    .unwrap();
    let file_request = serde_json::from_value::<ServerRequest>(serde_json::json!({
        "method": "item/fileChange/requestApproval",
        "id": 2,
        "params": {
            "threadId": "thread-1",
            "turnId": "turn-1",
            "itemId": "item-2",
            "cwd": "/workspace",
            "changes": [],
            "startedAtMs": 1
        }
    }))
    .unwrap();
    let permissions_request = serde_json::from_value::<ServerRequest>(serde_json::json!({
        "method": "item/permissions/requestApproval",
        "id": 3,
        "params": {
            "threadId": "thread-1",
            "turnId": "turn-1",
            "itemId": "item-3",
            "cwd": "/workspace",
            "startedAtMs": 1,
            "permissions": {
                "network": { "enabled": true }
            }
        }
    }))
    .unwrap();
    let current_time_request = ServerRequest::CurrentTimeRead {
        id: codex_app_server_client::protocol::RequestId::Int64(4),
        params: CurrentTimeReadParams {
            thread_id: "thread-1".to_owned(),
        },
    };

    let allow_all = AllowAllApprovalHandler;
    let read_only = ReadOnlyApprovalHandler;

    assert_eq!(
        reply_result_json(allow_all.handle(&command_request)),
        serde_json::json!({ "decision": "accept" })
    );
    assert_eq!(
        reply_result_json(allow_all.handle(&file_request)),
        serde_json::json!({ "decision": "accept" })
    );
    assert_eq!(
        reply_result_json(allow_all.handle(&permissions_request)),
        serde_json::json!({
            "permissions": { "network": { "enabled": true } },
            "scope": "turn"
        })
    );
    assert_eq!(
        reply_result_json(read_only.handle(&command_request)),
        serde_json::json!({ "decision": "decline" })
    );
    assert_eq!(
        reply_result_json(read_only.handle(&file_request)),
        serde_json::json!({ "decision": "decline" })
    );
    assert!(reply_result_json(read_only.handle(&current_time_request))["currentTimeAt"].is_i64());
}

async fn respond_to_initialize<R, W>(reader: &mut R, writer: &mut W)
where
    R: tokio::io::AsyncBufRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let init = read_json_line(reader).await;
    assert_eq!(init["method"], "initialize");
    respond_json(
        writer,
        init["id"].clone(),
        serde_json::json!({
            "codexHome": "/tmp/codex-home",
            "platformFamily": "unix",
            "platformOs": "linux",
            "userAgent": "fake-codex"
        }),
    )
    .await;

    let initialized = read_json_line(reader).await;
    assert_eq!(initialized["method"], "initialized");
}

async fn read_json_line<R>(reader: &mut R) -> serde_json::Value
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    serde_json::from_str(&line).unwrap()
}

async fn respond_json<W>(writer: &mut W, id: serde_json::Value, result: serde_json::Value)
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let response = serde_json::json!({ "id": id, "result": result });
    writer
        .write_all(format!("{response}\n").as_bytes())
        .await
        .unwrap();
}

async fn write_notification<W>(writer: &mut W, notification: serde_json::Value)
where
    W: tokio::io::AsyncWrite + Unpin,
{
    writer
        .write_all(format!("{notification}\n").as_bytes())
        .await
        .unwrap();
}

fn fake_thread_start_response(thread_id: &str, model: &str) -> serde_json::Value {
    serde_json::json!({
        "approvalPolicy": "never",
        "approvalsReviewer": "user",
        "cwd": "/workspace",
        "model": model,
        "modelProvider": "openai",
        "sandbox": { "type": "readOnly", "networkAccess": false },
        "thread": {
            "cliVersion": "codex-cli 0.144.3",
            "createdAt": 1,
            "cwd": "/workspace",
            "ephemeral": true,
            "historyMode": "paginated",
            "id": thread_id,
            "modelProvider": "openai",
            "preview": "",
            "sessionId": "session-1",
            "source": "appServer",
            "status": { "type": "idle" },
            "turns": [],
            "updatedAt": 1
        }
    })
}

fn reply_result_json(reply: ServerRequestReply) -> serde_json::Value {
    match reply {
        ServerRequestReply::Result(value) => value,
        ServerRequestReply::Error { message, .. } => panic!("unexpected error reply: {message}"),
    }
}
