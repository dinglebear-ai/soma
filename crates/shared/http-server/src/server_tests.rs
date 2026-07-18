//! Acceptance test: a router that knows nothing about Soma is served end to
//! end through `soma-http-server`'s listener + run-loop plumbing.

use std::net::SocketAddr;
use std::time::Duration;

use axum::{routing::get, Router};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::*;

/// A router with no Soma types anywhere in it — the point of this crate is
/// that it doesn't need any.
fn fake_unrelated_router() -> Router {
    Router::new().route("/ping", get(|| async { "pong" }))
}

async fn get_raw(addr: SocketAddr, path: &str) -> String {
    let mut stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr))
        .await
        .expect("connect timed out")
        .expect("connect failed");
    let request = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write failed");
    let mut buf = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut buf))
        .await
        .expect("read timed out")
        .expect("read failed");
    String::from_utf8(buf).expect("response was not utf-8")
}

#[tokio::test]
async fn fake_router_is_served_through_soma_http_server() {
    let listener = bind("127.0.0.1:0")
        .await
        .expect("bind should succeed on an ephemeral port");
    let addr = listener.local_addr().expect("listener has a local addr");

    let server = tokio::spawn(serve(listener, fake_unrelated_router()));

    let response = get_raw(addr, "/ping").await;
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "unexpected status line in response:\n{response}"
    );
    assert!(
        response.ends_with("pong"),
        "expected body \"pong\", got response:\n{response}"
    );

    server.abort();
}

#[tokio::test]
async fn fake_router_survives_graceful_shutdown_wiring() {
    let listener = bind("127.0.0.1:0")
        .await
        .expect("bind should succeed on an ephemeral port");
    let addr = listener.local_addr().expect("listener has a local addr");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let shutdown = async move {
        let _ = shutdown_rx.await;
    };
    let server = tokio::spawn(serve_with_shutdown(
        listener,
        fake_unrelated_router(),
        shutdown,
    ));

    let response = get_raw(addr, "/ping").await;
    assert!(response.starts_with("HTTP/1.1 200"));

    // Ask the server to drain and stop; it should exit cleanly rather than
    // hang or error.
    let _ = shutdown_tx.send(());
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("server did not shut down in time")
        .expect("server task panicked")
        .expect("server loop returned an error");
}

#[tokio::test]
async fn shutdown_drains_an_in_flight_request_instead_of_cutting_it_off() {
    // The previous test only proves the server exits cleanly once every
    // request has already finished — it would still pass under a hard-abort
    // shutdown that never actually waits for in-flight work. This test
    // proves the "graceful" part: a request already being handled when the
    // shutdown signal fires must still complete successfully, not be cut
    // off mid-response.
    let slow_router = Router::new().route(
        "/slow",
        get(|| async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            "slow-done"
        }),
    );

    let listener = bind("127.0.0.1:0")
        .await
        .expect("bind should succeed on an ephemeral port");
    let addr = listener.local_addr().expect("listener has a local addr");

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let shutdown = async move {
        let _ = shutdown_rx.await;
    };
    let server = tokio::spawn(serve_with_shutdown(listener, slow_router, shutdown));

    // Start the slow request on its own task so we can trigger shutdown
    // while it's still in flight.
    let request_task = tokio::spawn(async move { get_raw(addr, "/slow").await });

    // Give the connection time to be accepted and the handler to start
    // sleeping, then fire shutdown mid-request — well before the handler's
    // 300ms sleep elapses.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let _ = shutdown_tx.send(());

    let response = tokio::time::timeout(Duration::from_secs(5), request_task)
        .await
        .expect("in-flight request did not complete in time")
        .expect("request task panicked");
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "in-flight request must still succeed across a graceful shutdown, got:\n{response}"
    );
    assert!(
        response.ends_with("slow-done"),
        "in-flight request's response body must not be cut off, got:\n{response}"
    );

    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("server did not shut down in time")
        .expect("server task panicked")
        .expect("server loop returned an error");
}

#[test]
fn bind_error_display_names_the_address() {
    let error = ServerError::Bind {
        addr: "127.0.0.1:0".to_owned(),
        source: std::io::Error::new(std::io::ErrorKind::AddrInUse, "in use"),
    };
    assert!(error.to_string().contains("127.0.0.1:0"));
}

#[tokio::test]
async fn bind_accepts_a_host_port_string_like_config_supplies() {
    // `McpConfig::bind_addr()` returns a `"{host}:{port}"` `String` (which
    // may name a hostname, not just an IP literal) — `bind` must accept the
    // same shape `tokio::net::TcpListener::bind` always did.
    let listener = bind(String::from("127.0.0.1:0"))
        .await
        .expect("bind should accept an owned host:port String");
    assert!(listener.local_addr().is_ok());
}
