use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

use super::*;
use crate::transport::Method;

/// Spawns a fake Incus daemon on a Unix socket in a temp dir. `responder`
/// receives the raw request bytes for each accepted connection and returns
/// the raw response bytes to write back before closing that connection.
pub(crate) async fn spawn_fake_daemon<F>(responder: F) -> (std::path::PathBuf, tempfile::TempDir)
where
    F: Fn(Vec<u8>) -> Vec<u8> + Send + Sync + 'static,
{
    let dir = tempfile::tempdir().expect("create temp dir");
    let socket_path = dir.path().join("incus.sock");
    let listener = UnixListener::bind(&socket_path).expect("bind unix listener");
    let responder = std::sync::Arc::new(responder);

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let responder = responder.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                buf.truncate(n);
                let response = responder(buf);
                let _ = stream.write_all(&response).await;
                let _ = stream.shutdown().await;
            });
        }
    });

    (socket_path, dir)
}

pub(crate) fn json_response(status_line: &str, body: &str) -> Vec<u8> {
    format!(
        "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

#[tokio::test]
async fn single_round_trip_sends_request_and_parses_response() {
    let body =
        r#"{"type":"sync","status":"Success","status_code":200,"metadata":{"hello":"world"}}"#;
    let (socket_path, _dir) =
        spawn_fake_daemon(move |_req| json_response("HTTP/1.1 200 OK", body)).await;

    let response = execute(
        &socket_path,
        RequestSpec {
            method: Method::Get,
            path: "/1.0/test",
            query: &[],
            body: None,
            if_match: None,
        },
        None,
    )
    .await
    .expect("round trip should succeed");

    assert_eq!(response.status, 200);
    let parsed: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(parsed["metadata"]["hello"], "world");
}

#[tokio::test]
async fn concurrent_requests_on_the_same_socket_do_not_block_each_other() {
    let body = r#"{"type":"sync","status":"Success","status_code":200,"metadata":{}}"#;
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        let req_text = String::from_utf8_lossy(&req);
        // The "slow" request path deliberately sleeps before responding,
        // simulating a long-poll wait_for_operation call. If requests were
        // serialized through one shared connection, the fast request below
        // would have to wait for this sleep to elapse too.
        if req_text.contains("/1.0/slow") {
            std::thread::sleep(Duration::from_millis(300));
        }
        json_response("HTTP/1.1 200 OK", body)
    })
    .await;

    let fast_path = socket_path.clone();
    let fast = tokio::spawn(async move {
        let start = std::time::Instant::now();
        execute(
            &fast_path,
            RequestSpec {
                method: Method::Get,
                path: "/1.0/fast",
                query: &[],
                body: None,
                if_match: None,
            },
            None,
        )
        .await
        .expect("fast request should succeed");
        start.elapsed()
    });

    // Give the slow request a head start so it's genuinely in-flight first.
    tokio::time::sleep(Duration::from_millis(20)).await;
    let slow_path = socket_path.clone();
    let _slow = tokio::spawn(async move {
        execute(
            &slow_path,
            RequestSpec {
                method: Method::Get,
                path: "/1.0/slow",
                query: &[],
                body: None,
                if_match: None,
            },
            None,
        )
        .await
    });

    let fast_elapsed = fast.await.expect("fast task should not panic");
    assert!(
        fast_elapsed < Duration::from_millis(150),
        "fast request took {fast_elapsed:?}, expected it to complete well before the slow \
         request's 300ms sleep - a shared/serialized connection would have blocked it"
    );
}

#[tokio::test]
async fn mid_response_disconnect_returns_transport_error_not_a_hang() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let socket_path = dir.path().join("incus.sock");
    let listener = UnixListener::bind(&socket_path).expect("bind unix listener");

    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut buf = vec![0u8; 8192];
        let _ = stream.read(&mut buf).await;
        // Write a truncated response: a Content-Length promising 100 bytes,
        // but the connection is closed after only 10 body bytes arrive.
        let partial = b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n0123456789";
        let _ = stream.write_all(partial).await;
        let _ = stream.shutdown().await;
    });

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        execute(
            &socket_path,
            RequestSpec {
                method: Method::Get,
                path: "/1.0/test",
                query: &[],
                body: None,
                if_match: None,
            },
            None,
        ),
    )
    .await
    .expect("must not hang - should return promptly with an error");

    assert!(
        matches!(result, Err(crate::Error::Transport(_))),
        "expected Error::Transport for a truncated body, got {result:?}"
    );
}

#[tokio::test]
async fn execute_rejects_crlf_injection_in_path_before_sending_anything() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        *seen.lock().unwrap() = String::from_utf8_lossy(&req).into_owned();
        json_response(
            "HTTP/1.1 200 OK",
            r#"{"type":"sync","status":"Success","status_code":200,"metadata":{}}"#,
        )
    })
    .await;

    // An instance name (or any other caller-supplied path segment) carrying
    // an embedded CRLF sequence could smuggle a second, fully
    // attacker-controlled request onto the wire if it weren't rejected.
    let malicious_path =
        "/1.0/instances/c1\r\n\r\nDELETE /1.0/instances/other HTTP/1.1\r\nHost: localhost\r\n\r\n";

    let result = execute(
        &socket_path,
        RequestSpec {
            method: Method::Get,
            path: malicious_path,
            query: &[],
            body: None,
            if_match: None,
        },
        None,
    )
    .await;

    assert!(
        matches!(result, Err(crate::Error::InvalidRequest(_))),
        "expected InvalidRequest for a CRLF-injecting path, got {result:?}"
    );
    assert!(
        seen_request.lock().unwrap().is_empty(),
        "the malicious request must never reach the wire - the daemon should never even see a \
         connection"
    );
}

#[tokio::test]
async fn execute_rejects_crlf_injection_in_if_match_before_sending_anything() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        *seen.lock().unwrap() = String::from_utf8_lossy(&req).into_owned();
        json_response(
            "HTTP/1.1 200 OK",
            r#"{"type":"sync","status":"Success","status_code":200,"metadata":{}}"#,
        )
    })
    .await;

    let malicious_etag =
        "\"abc\"\r\n\r\nDELETE /1.0/instances/c1 HTTP/1.1\r\nHost: localhost\r\n\r\n";

    let result = execute(
        &socket_path,
        RequestSpec {
            method: Method::Get,
            path: "/1.0/instances/c1",
            query: &[],
            body: None,
            if_match: Some(malicious_etag),
        },
        None,
    )
    .await;

    assert!(
        matches!(result, Err(crate::Error::InvalidRequest(_))),
        "expected InvalidRequest for a CRLF-injecting If-Match value, got {result:?}"
    );
    assert!(
        seen_request.lock().unwrap().is_empty(),
        "the malicious request must never reach the wire"
    );
}

#[tokio::test]
async fn constructing_client_with_a_non_socket_path_fails_fast() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let regular_file = dir.path().join("not-a-socket.txt");
    std::fs::write(&regular_file, b"hello").expect("write regular file");

    let result = check_is_socket(&regular_file);
    assert!(
        result.is_err(),
        "a regular file must not be accepted as a socket path"
    );
}

#[tokio::test]
async fn response_exceeding_the_cap_is_rejected_without_buffering_it_fully() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let socket_path = dir.path().join("incus.sock");
    let listener = UnixListener::bind(&socket_path).expect("bind unix listener");

    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut buf = vec![0u8; 8192];
        let _ = stream.read(&mut buf).await;
        // Claim a body far larger than the test's cap; the transport should
        // reject based on the Content-Length header alone, before reading.
        let headers = b"HTTP/1.1 200 OK\r\nContent-Length: 999999999\r\n\r\n";
        let _ = stream.write_all(headers).await;
        // Deliberately never write the (huge) body - if the implementation
        // tried to read it all, this test would hang until the 5s timeout.
    });

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        execute_capped(
            &socket_path,
            RequestSpec {
                method: Method::Get,
                path: "/1.0/test",
                query: &[],
                body: None,
                if_match: None,
            },
            1024,
            None,
        ),
    )
    .await
    .expect("must reject based on Content-Length before attempting to read the body");

    assert!(matches!(
        result,
        Err(crate::Error::ResponseTooLarge { limit: 1024 })
    ));
}

/// Builds a `Transfer-Encoding: chunked` response body from `chunks`,
/// terminated by the standard zero-size chunk.
fn chunked_response(status_line: &str, chunks: &[&str]) -> Vec<u8> {
    let mut out = format!("{status_line}\r\nTransfer-Encoding: chunked\r\n\r\n").into_bytes();
    for chunk in chunks {
        out.extend_from_slice(format!("{:x}\r\n", chunk.len()).as_bytes());
        out.extend_from_slice(chunk.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"0\r\n\r\n");
    out
}

#[tokio::test]
async fn chunked_transfer_encoding_response_decodes_correctly() {
    // Split across multiple chunks to exercise the multi-chunk accumulation
    // path, not just a single-chunk shortcut.
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| {
        chunked_response(
            "HTTP/1.1 200 OK",
            &[
                r#"{"type":"sync","status":"S"#,
                r#"uccess","status_code":200,"metadata":{"hello":"world"}}"#,
            ],
        )
    })
    .await;

    let response = execute(
        &socket_path,
        RequestSpec {
            method: Method::Get,
            path: "/1.0/test",
            query: &[],
            body: None,
            if_match: None,
        },
        None,
    )
    .await
    .expect("chunked round trip should succeed");

    assert_eq!(response.status, 200);
    let parsed: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(parsed["metadata"]["hello"], "world");
}

#[tokio::test]
async fn chunked_body_chunk_size_that_would_overflow_the_cap_check_is_rejected_not_panicking() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let socket_path = dir.path().join("incus.sock");
    let listener = UnixListener::bind(&socket_path).expect("bind unix listener");

    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut buf = vec![0u8; 8192];
        let _ = stream.read(&mut buf).await;
        // A chunk-size line claiming a value near usize::MAX. Before the
        // overflow fix, `body.len() + size` here would wrap (release) or
        // panic (debug) instead of being caught by the `> max_bytes` guard.
        let headers = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\n";
        let _ = stream.write_all(headers).await;
        // Deliberately never write the (impossible) chunk body.
    });

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        execute(
            &socket_path,
            RequestSpec {
                method: Method::Get,
                path: "/1.0/test",
                query: &[],
                body: None,
                if_match: None,
            },
            None,
        ),
    )
    .await
    .expect("must reject the oversized chunk-size claim promptly, not hang");

    assert!(
        matches!(result, Err(crate::Error::ResponseTooLarge { .. })),
        "expected ResponseTooLarge for a chunk-size claim near usize::MAX, got {result:?}"
    );
}

#[tokio::test]
async fn response_with_too_many_header_lines_is_rejected() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let socket_path = dir.path().join("incus.sock");
    let listener = UnixListener::bind(&socket_path).expect("bind unix listener");

    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let mut buf = vec![0u8; 8192];
        let _ = stream.read(&mut buf).await;
        // MAX_HEADER_COUNT is 100 - send well past that before ever
        // terminating the header section, so a peer that just keeps
        // sending small, individually-legal header lines can't grow
        // memory without bound.
        let mut response = b"HTTP/1.1 200 OK\r\n".to_vec();
        for i in 0..200 {
            response.extend_from_slice(format!("X-Filler-{i}: 1\r\n").as_bytes());
        }
        response.extend_from_slice(b"\r\n{}");
        let _ = stream.write_all(&response).await;
    });

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        execute(
            &socket_path,
            RequestSpec {
                method: Method::Get,
                path: "/1.0/test",
                query: &[],
                body: None,
                if_match: None,
            },
            None,
        ),
    )
    .await
    .expect("must reject the excessive header count promptly, not hang");

    assert!(
        matches!(result, Err(crate::Error::ResponseTooLarge { .. })),
        "expected ResponseTooLarge for a response with 200 header lines, got {result:?}"
    );
}

#[tokio::test]
async fn a_daemon_that_never_responds_times_out_instead_of_hanging_forever() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let socket_path = dir.path().join("incus.sock");
    let listener = UnixListener::bind(&socket_path).expect("bind unix listener");

    tokio::spawn(async move {
        let Ok((_stream, _)) = listener.accept().await else {
            return;
        };
        // Accept the connection but never read or write anything - the
        // caller-supplied timeout, not a server-side signal, must be what
        // ends this call.
        std::future::pending::<()>().await;
    });

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        execute(
            &socket_path,
            RequestSpec {
                method: Method::Get,
                path: "/1.0/test",
                query: &[],
                body: None,
                if_match: None,
            },
            Some(Duration::from_millis(100)),
        ),
    )
    .await
    .expect("the per-request timeout must fire well before the test's own 5s backstop");

    assert!(
        matches!(result, Err(crate::Error::Timeout { .. })),
        "expected Error::Timeout for a daemon that never responds, got {result:?}"
    );
}
