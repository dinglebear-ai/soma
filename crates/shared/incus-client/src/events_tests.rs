use futures::StreamExt;
use tokio::net::UnixListener;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;

use super::*;
use crate::config::ClientConfig;
use crate::transport::Client;

/// Spawns a fake Incus daemon that accepts one WebSocket connection on a
/// Unix socket and sends the given canned text frames before closing.
async fn spawn_fake_events_daemon(frames: Vec<String>) -> (std::path::PathBuf, tempfile::TempDir) {
    spawn_fake_events_daemon_with_close(frames, None).await
}

/// Like [`spawn_fake_events_daemon`], but lets the caller specify the close
/// frame sent after the canned text frames (`None` closes with no frame at
/// all, matching a clean end-of-stream).
async fn spawn_fake_events_daemon_with_close(
    frames: Vec<String>,
    close: Option<CloseFrame>,
) -> (std::path::PathBuf, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let socket_path = dir.path().join("incus.sock");
    let listener = UnixListener::bind(&socket_path).expect("bind unix listener");

    tokio::spawn(async move {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let mut ws = tokio_tungstenite::accept_async(stream)
            .await
            .expect("accept websocket handshake");
        for frame in frames {
            use futures::SinkExt;
            let _ = ws.send(Message::Text(frame.into())).await;
        }
        let _ = ws.close(close).await;
    });

    (socket_path, dir)
}

#[tokio::test]
async fn subscribe_events_yields_a_typed_operation_event() {
    let frame = r#"{"type":"operation","metadata":{"id":"11111111-1111-1111-1111-111111111111","class":"task","status":"Success","status_code":200,"resources":{},"may_cancel":false,"err":null}}"#.to_owned();
    let (socket_path, _dir) = spawn_fake_events_daemon(vec![frame]).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let mut stream = client
        .subscribe_events(EventFilter::default())
        .await
        .expect("subscribe should succeed");

    let event = stream
        .next()
        .await
        .expect("stream should yield one event")
        .expect("event should parse successfully");

    match event {
        Event::Operation(op) => assert_eq!(op.status_code, 200),
        other => panic!("expected Event::Operation, got {other:?}"),
    }
}

#[tokio::test]
async fn subscribe_events_yields_typed_lifecycle_and_logging_events() {
    let lifecycle = r#"{"type":"lifecycle","metadata":{"action":"instance-started","source":"/1.0/instances/c1"}}"#.to_owned();
    let logging = r#"{"type":"logging","metadata":{"message":"hello","level":"info"}}"#.to_owned();
    let (socket_path, _dir) = spawn_fake_events_daemon(vec![lifecycle, logging]).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let mut stream = client
        .subscribe_events(EventFilter::default())
        .await
        .expect("subscribe should succeed");

    let first = stream.next().await.unwrap().unwrap();
    assert!(matches!(first, Event::Lifecycle(_)));
    let second = stream.next().await.unwrap().unwrap();
    assert!(matches!(second, Event::Logging(_)));
}

#[tokio::test]
async fn subscribe_events_surfaces_an_abnormal_close_as_an_error_not_a_silent_end() {
    let (socket_path, _dir) = spawn_fake_events_daemon_with_close(
        vec![],
        Some(CloseFrame {
            code: CloseCode::Error,
            reason: "daemon restarting".into(),
        }),
    )
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let mut stream = client
        .subscribe_events(EventFilter::default())
        .await
        .expect("subscribe should succeed");

    let first = stream
        .next()
        .await
        .expect("an abnormal close must surface as one Err item, not end the stream silently");
    assert!(
        matches!(first, Err(crate::Error::InvalidResponse(_))),
        "expected InvalidResponse for a non-Normal close code, got {first:?}"
    );

    // After the abnormal-close error item, the stream ends (the underlying
    // connection really is closed) - this proves the error doesn't loop
    // forever re-observing the same close frame.
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn subscribe_events_ends_silently_on_a_normal_close() {
    let frame = r#"{"type":"lifecycle","metadata":{"action":"instance-started"}}"#.to_owned();
    let (socket_path, _dir) = spawn_fake_events_daemon_with_close(
        vec![frame],
        Some(CloseFrame {
            code: CloseCode::Normal,
            reason: "".into(),
        }),
    )
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let mut stream = client
        .subscribe_events(EventFilter::default())
        .await
        .expect("subscribe should succeed");

    let first = stream.next().await.unwrap();
    assert!(first.is_ok(), "the lifecycle event should parse cleanly");
    assert!(
        stream.next().await.is_none(),
        "a Normal close code must end the stream silently, not as an Err item"
    );
}

#[tokio::test]
async fn subscribe_events_distinguishes_websocket_protocol_errors_from_io_errors() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let socket_path = dir.path().join("incus.sock");
    let listener = UnixListener::bind(&socket_path).expect("bind unix listener");

    tokio::spawn(async move {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let mut ws = tokio_tungstenite::accept_async(stream)
            .await
            .expect("accept websocket handshake");
        // Write a raw WebSocket text frame carrying invalid UTF-8 directly
        // to the underlying stream, bypassing tungstenite's own encoder -
        // `Message::Text` can't be constructed with invalid UTF-8 through
        // its safe API (it's backed by a real `String`), so this is the
        // only way to trigger a genuine protocol-level parse failure rather
        // than a socket I/O error. Frame: FIN + text opcode (0x81),
        // unmasked 2-byte payload (0x02), payload bytes that aren't valid
        // UTF-8 (0xFF 0xFE).
        use tokio::io::AsyncWriteExt;
        let raw_frame: &[u8] = &[0x81, 0x02, 0xff, 0xfe];
        let _ = ws.get_mut().write_all(raw_frame).await;
        let _ = ws.get_mut().flush().await;
        // Keep the connection open long enough for the client to read it.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    });

    let client = Client::new(ClientConfig::unix_socket(socket_path));
    let mut stream = client
        .subscribe_events(EventFilter::default())
        .await
        .expect("subscribe should succeed");

    let first = stream
        .next()
        .await
        .expect("must yield one Err item for the malformed frame");
    assert!(
        matches!(first, Err(crate::Error::WebSocketProtocol(_))),
        "expected WebSocketProtocol (not Transport) for an invalid-UTF8 text frame, got {first:?}"
    );
}

#[test]
fn event_filter_default_subscribes_to_everything() {
    let filter = EventFilter::default();
    assert!(filter.operations);
    assert!(filter.lifecycle);
    assert!(filter.logging);
}
