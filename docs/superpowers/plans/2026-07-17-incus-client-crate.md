# Incus REST API Client Crate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `crates/shared/incus-client`, a pure async Rust client library over a local Unix-socket connection to the Incus REST API (system container/VM manager), covering operation polling, WebSocket events, and CRUD for instances/images/networks/storage/projects.

**Architecture:** A hand-rolled minimal HTTP/1.1 client over `tokio::net::UnixStream` (mirroring this workspace's existing NDJSON-over-`UnixStream` framing precedent in `codex-app-server-client`, opening one fresh connection per request rather than pooling — cheap and simple for a local Unix socket, and it trivially satisfies concurrent-request independence) sits behind one `Client::request()` entry point that parses Incus's sync/async/error JSON envelope. Every resource module (instances, images, networks, storage, projects) and the `operations`/`events` modules call through that one method; nothing below it, and nothing above it, does its own HTTP framing.

**Tech Stack:** Rust 2021 (rust-version 1.96), tokio (async runtime), serde/serde_json (wire types), thiserror (errors), uuid (operation IDs), tokio-tungstenite (WebSocket events, feature-gated), wiremock (HTTP-mock tests — used only for envelope-parsing tests since the real transport is Unix-socket, not TCP; see Task 2's testing notes), tempfile (UDS test fixtures).

## Global Constraints

- Package `incus-client` at `crates/shared/incus-client`, `edition = "2021"`, `rust-version = "1.96"`, `version = "0.1.0"`, `publish = false`, `license = "MIT"`. `[package.metadata.soma-architecture] layer = "shared"`.
- Zero path-dependencies on other crates in this workspace — external, published dependencies only.
- `[features] default = []`. Exactly one optional feature this epic: `events` (gates `tokio-tungstenite` and `src/events.rs`). No `mtls` feature — remote HTTPS/TOFU/trust-token transport and full certificates CRUD are out of scope for this epic entirely (tracked in beads epic `rmcp-template-21b7` for whenever a real remote consumer exists; do not implement any of that surface here).
- Every `.rs` file with non-trivial logic gets a sibling `_tests.rs` file wired via `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;` at the bottom of `foo.rs` — copy this exact idiom from `crates/shared/mcp/client/src/config.rs:287-289`. Never `foo/mod.rs`.
- Crate-wide mutation-return convention: every resource method that triggers a documented-as-async Incus operation returns `crate::operations::Operation`. If unsure whether a given endpoint is sync or async, treat it as async. No per-resource exceptions.
- Every list method takes an explicit `recursion: bool` parameter — no method may pick an implicit default. Doc-comment on every such method: `recursion = true` fetches every object's full body (config/devices/state) in one call and can be expensive on hosts with many objects; `recursion = false` returns lightweight name/URL references only.
- All response bodies (HTTP-over-Unix-socket and WebSocket) are capped at `MAX_RESPONSE_BYTES = 64 * 1024 * 1024` (64 MiB) — mirrors `codex-app-server-client`'s `MAX_LINE_BYTES` precedent (`crates/shared/codex-app-server-client/src/transport.rs:20`). Exceeding the cap is a distinct `Error` variant, never a silent truncation or unbounded read.
- No secrets flow through this crate in this epic (unix-socket transport has no auth material) — this constraint is why the follow-up epic exists for the token/cert-bearing remote transport.
- Testing is entirely synthetic this epic: a real `tokio::net::UnixListener` in a temp dir for transport tests, and `wiremock` for a small number of pure envelope-parsing unit tests that don't need a real socket. No real Incus daemon anywhere in this plan's test suite — a downstream consumer should treat this crate as contract-tested, not integration-tested, until a real-daemon smoke test exists (not scoped here).
- Run `cargo fmt -p incus-client`, `cargo clippy -p incus-client --all-features -- -D warnings`, and `cargo test -p incus-client --all-features` (plus once more with default features, no `--all-features`, to confirm the `events`-gated code doesn't leak into the default build) at the end of every task, not just once at the end of the plan.

## File Structure

```
crates/shared/incus-client/
├── Cargo.toml
└── src/
    ├── lib.rs                       # crate root: doc comment, pub mod wiring, re-exports
    ├── error.rs                     # Error enum (thiserror), Result alias
    ├── error_tests.rs
    ├── config.rs                    # ClientConfig::unix_socket(path)
    ├── config_tests.rs
    ├── transport.rs                 # Client, Method, IncusEnvelope, WithEtag<T>, request()
    ├── transport/
    │   ├── unix.rs                  # hand-rolled HTTP/1.1-over-UnixStream, fresh conn/request
    │   └── unix_tests.rs
    ├── operations.rs                # Operation, OperationClass, wait_for_operation, cancel_operation
    ├── operations_tests.rs
    ├── events.rs                    # (behind `events` feature) Event, subscribe_events
    ├── events_tests.rs
    ├── resources.rs                 # mod declarations only
    └── resources/
        ├── instances.rs             # Instance CRUD + lifecycle + snapshots
        ├── instances_tests.rs
        ├── images.rs                # Image CRUD
        ├── images_tests.rs
        ├── networks.rs              # Network CRUD
        ├── networks_tests.rs
        ├── storage.rs               # StoragePool + StorageVolume CRUD
        ├── storage_tests.rs
        ├── projects.rs              # Project CRUD
        └── projects_tests.rs
```

Six tasks, strictly sequential (each depends on types/methods the previous task defines) except Task 5 and Task 6, which can run in parallel once Task 4 lands (they touch disjoint files).

---

## Task 1: Crate Scaffolding, Error Types, Config

**Files:**
- Modify: `Cargo.toml` (workspace root) — add `"crates/shared/incus-client"` to `[workspace] members`, alphabetically after `"crates/shared/codex-app-server-client"` and before `"crates/shared/mcp/client"`.
- Create: `crates/shared/incus-client/Cargo.toml`
- Create: `crates/shared/incus-client/src/lib.rs`
- Create: `crates/shared/incus-client/src/error.rs`
- Create: `crates/shared/incus-client/src/error_tests.rs`
- Create: `crates/shared/incus-client/src/config.rs`
- Create: `crates/shared/incus-client/src/config_tests.rs`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Produces: `pub enum Error { Transport(std::io::Error), Api { status_code: u16, message: String }, Serialization(serde_json::Error), InvalidResponse(String), ResponseTooLarge { limit: usize }, NotCancellable, OperationFailed { id: uuid::Uuid, status_code: u16, err: Option<String> }, PreconditionFailed { resource: String } }`, `pub type Result<T> = std::result::Result<T, Error>`, `pub struct ClientConfig { pub(crate) socket_path: std::path::PathBuf }` with `ClientConfig::unix_socket(path: impl Into<std::path::PathBuf>) -> Self`.

- [ ] **Step 1: Add the workspace member**

Read the root `Cargo.toml` `[workspace] members` array first to confirm exact current ordering, then edit it.

```toml
[workspace]
members = [
  "apps/soma",
  "crates/shared/auth",
  "crates/shared/codemode",
  "crates/shared/codex-app-server-client",
  "crates/shared/incus-client",
  "crates/shared/mcp/client",
  "crates/shared/mcp/gateway",
  "crates/shared/mcp/proxy",
  "crates/shared/mcp/server",
  "crates/shared/observability",
  "crates/shared/openapi",
  "crates/shared/provider-core",
  "crates/shared/traces",
  "crates/soma/api",
  "crates/soma/application",
  "crates/soma/cli",
  "crates/soma/contracts",
  "crates/soma/domain",
  "crates/soma/mcp",
  "crates/soma/runtime",
  "crates/soma/service",
  "crates/soma/test-support",
  "crates/soma/web",
  "xtask",
]
resolver = "2"
```

- [ ] **Step 2: Create the crate manifest**

```bash
mkdir -p crates/shared/incus-client/src/transport crates/shared/incus-client/src/resources
```

Create `crates/shared/incus-client/Cargo.toml`:

```toml
[package]
name = "incus-client"
version = "0.1.0"
edition = "2021"
rust-version = "1.96"
authors.workspace = true
description = "Async Rust client for the Incus REST API (system container and VM management)."
homepage.workspace = true
license = "MIT"
repository.workspace = true
publish = false

[package.metadata.soma-architecture]
layer = "shared"

# Deliberately has zero path-dependencies on other crates in this workspace -
# this is a standalone protocol client, like codex-app-server-client.

[features]
# Explicit empty default to keep Cargo feature unification a no-op for
# downstream consumers.
default = []
# WebSocket subscription to Incus's /1.0/events push-notification stream.
# wait_for_operation() works without this feature; it's an enhancement, not
# a requirement, for operation-completion tracking.
events = ["dep:tokio-tungstenite"]

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tokio = { version = "1", features = ["rt", "macros", "sync", "time"] }
tokio-tungstenite = { version = "0.29", features = ["rustls-tls-webpki-roots"], optional = true }
tracing = "0.1"
url = "2"
uuid = { version = "1", features = ["serde"] }

[dev-dependencies]
tempfile = "3"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
wiremock = "0.6"
```

Note: `tokio` here deliberately omits `"net"` and `"io-util"` — Task 2 adds them when it introduces the Unix-socket transport that actually needs them. Do not pre-add features a later task owns.

- [ ] **Step 3: Write the crate root**

Create `crates/shared/incus-client/src/lib.rs`:

```rust
//! Async Rust client for the [Incus REST API](https://linuxcontainers.org/incus/docs/main/rest-api/)
//! (system container and VM management).
//!
//! This crate speaks to the Incus daemon over a **local Unix domain socket
//! only**. Incus also supports a remote mutual-TLS HTTPS transport with a
//! trust-on-first-use certificate model, but that surface is *not*
//! implemented here — it's tracked as a separate follow-up epic pending a
//! real remote consumer, since a from-scratch TLS trust implementation
//! carries real security risk that isn't worth taking on speculatively.
//!
//! Every operation-returning mutation surfaces a [`operations::Operation`]
//! rather than assuming synchronous completion — see
//! [`operations::Client::wait_for_operation`] for the recommended way to
//! wait for one to finish.

pub mod config;
pub mod error;
pub mod operations;
pub mod resources;
pub mod transport;

#[cfg(feature = "events")]
pub mod events;

pub use config::ClientConfig;
pub use error::{Error, Result};
pub use transport::Client;
```

- [ ] **Step 4: Write the error type**

Create `crates/shared/incus-client/src/error.rs`:

```rust
/// Errors returned by [`crate::Client`] and every resource method built on
/// top of it.
///
/// `#[non_exhaustive]` because this crate expects to grow new failure modes
/// as more of the Incus API surface is covered - matching exhaustively on
/// this enum today would make every future variant addition a breaking
/// change for downstream crates.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("transport I/O error: {0}")]
    Transport(#[from] std::io::Error),

    #[error("Incus API error (status {status_code}): {message}")]
    Api { status_code: u16, message: String },

    #[error("failed to (de)serialize a request or response body: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("response did not match any known Incus envelope shape: {0}")]
    InvalidResponse(String),

    #[error("response body exceeded the {limit}-byte cap")]
    ResponseTooLarge { limit: usize },

    #[error("operation is not cancellable (may_cancel is false)")]
    NotCancellable,

    #[error("operation {id} failed (status {status_code}): {}", err.as_deref().unwrap_or("no error message"))]
    OperationFailed {
        id: uuid::Uuid,
        status_code: u16,
        err: Option<String>,
    },

    #[error("precondition failed updating {resource} (stale ETag - re-fetch and retry)")]
    PreconditionFailed { resource: String },
}

pub type Result<T> = std::result::Result<T, Error>;
```

- [ ] **Step 5: Write the error tests**

Create `crates/shared/incus-client/src/error_tests.rs`:

```rust
use super::*;

#[test]
fn api_error_display_includes_status_and_message() {
    let err = Error::Api {
        status_code: 404,
        message: "not found".to_owned(),
    };
    let text = err.to_string();
    assert!(text.contains("404"));
    assert!(text.contains("not found"));
}

#[test]
fn operation_failed_display_falls_back_when_err_is_none() {
    let id = uuid::Uuid::nil();
    let err = Error::OperationFailed {
        id,
        status_code: 400,
        err: None,
    };
    assert!(err.to_string().contains("no error message"));
}

#[test]
fn operation_failed_display_includes_err_when_present() {
    let id = uuid::Uuid::nil();
    let err = Error::OperationFailed {
        id,
        status_code: 400,
        err: Some("storage pool full".to_owned()),
    };
    assert!(err.to_string().contains("storage pool full"));
}

#[test]
fn serialization_error_converts_via_from() {
    let json_err = serde_json::from_str::<serde_json::Value>("{not json")
        .expect_err("deliberately malformed JSON");
    let err: Error = json_err.into();
    assert!(matches!(err, Error::Serialization(_)));
}

#[test]
fn io_error_converts_via_from() {
    let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "boom");
    let err: Error = io_err.into();
    assert!(matches!(err, Error::Transport(_)));
}
```

Wire the sibling test file at the bottom of `error.rs`:

```rust
#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
```

- [ ] **Step 6: Write the config type**

Create `crates/shared/incus-client/src/config.rs`:

```rust
use std::path::PathBuf;

/// Connection configuration for [`crate::Client`].
///
/// This epic only supports a local Unix-socket target. A `remote(url)`
/// constructor for the mutual-TLS transport is intentionally absent - see
/// the crate root doc comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientConfig {
    pub(crate) socket_path: PathBuf,
}

impl ClientConfig {
    /// Configure a client that connects to the Incus daemon over the given
    /// Unix domain socket path (e.g. `/var/lib/incus/unix.socket`).
    #[must_use]
    pub fn unix_socket(path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: path.into(),
        }
    }
}
```

- [ ] **Step 7: Write the config tests**

Create `crates/shared/incus-client/src/config_tests.rs`:

```rust
use super::*;

#[test]
fn unix_socket_stores_the_given_path() {
    let config = ClientConfig::unix_socket("/var/lib/incus/unix.socket");
    assert_eq!(
        config.socket_path,
        PathBuf::from("/var/lib/incus/unix.socket")
    );
}

#[test]
fn unix_socket_accepts_owned_pathbuf_and_str() {
    let from_str = ClientConfig::unix_socket("/tmp/a.sock");
    let from_pathbuf = ClientConfig::unix_socket(PathBuf::from("/tmp/a.sock"));
    assert_eq!(from_str, from_pathbuf);
}
```

Wire the sibling test file at the bottom of `config.rs`:

```rust
#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
```

- [ ] **Step 8: Add stub `transport.rs`, `operations.rs`, `resources.rs` so the crate compiles**

Task 1 only needs the crate to build; Tasks 2-4 fill these in for real. Create minimal stubs now so `cargo build` succeeds at the end of this task:

Create `crates/shared/incus-client/src/transport.rs`:

```rust
//! Filled in by Task 2 (unix-socket transport layer).
```

Create `crates/shared/incus-client/src/operations.rs`:

```rust
//! Filled in by Task 3 (operation wait/cancel lifecycle).
```

Create `crates/shared/incus-client/src/resources.rs`:

```rust
//! Filled in by Task 4 (module skeleton) and Tasks 5-6 (resource CRUD).
```

`lib.rs` already references `pub use transport::Client;` (Step 3) which does not exist yet — remove that one re-export line for now and add it back in Task 2 once `Client` is defined, so this task's `cargo build` actually passes:

Edit `crates/shared/incus-client/src/lib.rs`, change:

```rust
pub use config::ClientConfig;
pub use error::{Error, Result};
pub use transport::Client;
```

to:

```rust
pub use config::ClientConfig;
pub use error::{Error, Result};
```

(Task 2 re-adds the `Client` re-export line once `transport.rs` defines it.)

- [ ] **Step 9: Build and test**

Run: `cargo build -p incus-client`
Expected: builds with zero warnings.

Run: `cargo test -p incus-client`
Expected: all tests in `error_tests.rs` and `config_tests.rs` pass (7 tests total).

Run: `cargo clippy -p incus-client -- -D warnings`
Expected: no warnings.

Run: `cargo fmt -p incus-client -- --check`
Expected: no diff (run `cargo fmt -p incus-client` first if it fails, then re-check).

Run: `cargo build --workspace`
Expected: the whole workspace still builds (confirms the new member doesn't break anything else).

- [ ] **Step 10: Update the changelog**

Edit `CHANGELOG.md`, add under `## [Unreleased]` → `### Added`:

```markdown
- Add a new `incus-client` crate: an async Rust client for the Incus REST API
  (system container/VM management) over a local Unix socket, starting with
  crate scaffolding and error types.
```

- [ ] **Step 11: Commit**

```bash
git add Cargo.toml crates/shared/incus-client CHANGELOG.md
git commit -m "feat(incus-client): scaffold crate, error types, config"
```

---

## Task 2: Unix-Socket Transport Layer

**Files:**
- Modify: `crates/shared/incus-client/Cargo.toml` (add `tokio` `net`/`io-util` features)
- Modify: `crates/shared/incus-client/src/lib.rs` (re-add `pub use transport::Client;`)
- Create: `crates/shared/incus-client/src/transport.rs` (replaces Task 1's stub)
- Create: `crates/shared/incus-client/src/transport/unix.rs`
- Create: `crates/shared/incus-client/src/transport/unix_tests.rs`
- Create: `crates/shared/incus-client/src/transport_tests.rs`

**Interfaces:**
- Consumes: `crate::config::ClientConfig { socket_path: PathBuf }`, `crate::error::{Error, Result}` (Task 1).
- Produces: `pub struct Client(Arc<ClientInner>)` with `Client::new(config: ClientConfig) -> Client` (infallible - no I/O happens at construction, only when a request is made), `Client: Clone + Send + Sync + 'static`; `pub(crate) enum Method { Get, Post, Put, Patch, Delete }`; `pub(crate) enum IncusEnvelope { Sync { metadata: serde_json::Value, etag: Option<String> }, Async { operation_url: String, metadata: serde_json::Value } }`; `pub struct WithEtag<T> { pub value: T, pub etag: Option<String> }`; `pub(crate) async fn Client::request(&self, method: Method, path: &str, query: &[(&str, &str)], body: Option<&serde_json::Value>, if_match: Option<&str>) -> Result<IncusEnvelope>` — the **one** method every later task's resource/operation code calls to reach the daemon.

### Design notes (read before writing code)

This crate opens a **fresh `UnixStream` connection per request**, not a pooled/persistent one. That's a deliberate, locked decision: connecting to a local Unix socket is a cheap syscall (no TCP handshake, no TLS negotiation), so pooling buys little, and opening fresh per request makes "N concurrent requests must not block each other" trivially true — there's no shared connection state for one slow request (e.g. a long `wait_for_operation` poll) to serialize behind. A mutex-guarded single persistent stream was considered and explicitly rejected for exactly this reason.

Response parsing enforces `MAX_RESPONSE_BYTES = 64 * 1024 * 1024` regardless of whether the response uses `Content-Length` or `Transfer-Encoding: chunked` framing, mirroring `codex-app-server-client`'s `MAX_LINE_BYTES` pattern (`crates/shared/codex-app-server-client/src/transport.rs:20`).

Before connecting, the socket path is checked with `std::fs::metadata` to confirm it's actually a Unix socket (`file_type().is_socket()`), not a stale regular file or a symlink to somewhere unexpected — catches the common "wrong path" / "daemon not running yet" failure mode with a clear error instead of an opaque connection-refused.

- [ ] **Step 1: Add the transport dependencies**

Edit `crates/shared/incus-client/Cargo.toml`, change the `tokio` line:

```toml
tokio = { version = "1", features = ["rt", "macros", "sync", "time", "net", "io-util"] }
```

- [ ] **Step 2: Write the failing concurrency test first**

This is the test that would have caught a mutex-guarded-single-stream anti-pattern, so it's written before any transport code exists.

Create `crates/shared/incus-client/src/transport/unix_tests.rs`:

```rust
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

use super::*;
use crate::transport::Method;

/// Spawns a fake Incus daemon on a Unix socket in a temp dir. `responder`
/// receives the raw request bytes for each accepted connection and returns
/// the raw response bytes to write back before closing that connection.
async fn spawn_fake_daemon<F>(responder: F) -> (std::path::PathBuf, tempfile::TempDir)
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

fn json_response(status_line: &str, body: &str) -> Vec<u8> {
    format!(
        "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

#[tokio::test]
async fn single_round_trip_sends_request_and_parses_response() {
    let body = r#"{"type":"sync","status":"Success","status_code":200,"metadata":{"hello":"world"}}"#;
    let (socket_path, _dir) =
        spawn_fake_daemon(move |_req| json_response("HTTP/1.1 200 OK", body)).await;

    let response = execute(&socket_path, Method::Get, "/1.0/test", &[], None, None)
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
        execute(&fast_path, Method::Get, "/1.0/fast", &[], None, None)
            .await
            .expect("fast request should succeed");
        start.elapsed()
    });

    // Give the slow request a head start so it's genuinely in-flight first.
    tokio::time::sleep(Duration::from_millis(20)).await;
    let slow_path = socket_path.clone();
    let _slow = tokio::spawn(async move {
        execute(&slow_path, Method::Get, "/1.0/slow", &[], None, None).await
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
        execute(&socket_path, Method::Get, "/1.0/test", &[], None, None),
    )
    .await
    .expect("must not hang - should return promptly with an error");

    assert!(
        matches!(result, Err(crate::Error::Transport(_))),
        "expected Error::Transport for a truncated body, got {result:?}"
    );
}

#[tokio::test]
async fn constructing_client_with_a_non_socket_path_fails_fast() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let regular_file = dir.path().join("not-a-socket.txt");
    std::fs::write(&regular_file, b"hello").expect("write regular file");

    let result = check_is_socket(&regular_file);
    assert!(result.is_err(), "a regular file must not be accepted as a socket path");
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
        execute_capped(&socket_path, Method::Get, "/1.0/test", &[], None, None, 1024),
    )
    .await
    .expect("must reject based on Content-Length before attempting to read the body");

    assert!(matches!(result, Err(crate::Error::ResponseTooLarge { limit: 1024 })));
}
```

- [ ] **Step 3: Run the tests to verify they fail (compile error is expected)**

Run: `cargo test -p incus-client --test-threads=1 2>&1 | head -40`
Expected: compile failure — `execute`, `check_is_socket`, and `execute_capped` don't exist yet in `crate::transport::unix`.

- [ ] **Step 4: Implement the Unix-socket HTTP/1.1 client**

Create `crates/shared/incus-client/src/transport/unix.rs`:

```rust
//! Minimal hand-rolled HTTP/1.1 client over a Unix domain socket. Opens one
//! fresh `UnixStream` per request rather than pooling - see the module-level
//! design notes in the implementation plan this was built from
//! (`docs/superpowers/plans/2026-07-17-incus-client-crate.md`): a local
//! socket connect is cheap, and a fresh connection per request makes
//! concurrent requests trivially independent of each other, unlike a shared,
//! mutex-guarded stream would be.

use std::os::unix::fs::FileTypeExt;
use std::path::Path;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::error::{Error, Result};
use crate::transport::Method;

/// Hard cap on a response body, enforced regardless of framing
/// (`Content-Length` or chunked). Mirrors `codex-app-server-client`'s
/// `MAX_LINE_BYTES` precedent.
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024 * 1024;

/// A parsed HTTP response: status code, headers (lowercased names), and the
/// raw body bytes. Envelope (Incus sync/async/error JSON) parsing happens
/// one layer up, in `crate::transport`.
#[derive(Debug, Clone)]
pub(crate) struct RawResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl RawResponse {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// Confirms `path` is actually a Unix domain socket before we ever try to
/// connect to it, so a stale regular file or wrong path fails with a clear
/// error instead of an opaque connection-refused.
pub(crate) fn check_is_socket(path: &Path) -> Result<()> {
    let metadata = std::fs::metadata(path).map_err(Error::Transport)?;
    if !metadata.file_type().is_socket() {
        return Err(Error::Transport(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{} is not a Unix domain socket", path.display()),
        )));
    }
    Ok(())
}

/// Executes one HTTP request over a fresh connection to `socket_path`,
/// capping the response body at [`MAX_RESPONSE_BYTES`].
pub(crate) async fn execute(
    socket_path: &Path,
    method: Method,
    path: &str,
    query: &[(&str, &str)],
    body: Option<&[u8]>,
    if_match: Option<&str>,
) -> Result<RawResponse> {
    execute_capped(
        socket_path,
        method,
        path,
        query,
        body,
        if_match,
        MAX_RESPONSE_BYTES,
    )
    .await
}

/// [`execute`]'s implementation, parameterized over the cap so tests can
/// exercise the boundary condition without a 64 MiB fixture.
pub(crate) async fn execute_capped(
    socket_path: &Path,
    method: Method,
    path: &str,
    query: &[(&str, &str)],
    body: Option<&[u8]>,
    if_match: Option<&str>,
    max_response_bytes: usize,
) -> Result<RawResponse> {
    check_is_socket(socket_path)?;
    let stream = UnixStream::connect(socket_path)
        .await
        .map_err(Error::Transport)?;
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    let request_line = build_request_line(method, path, query);
    write_half
        .write_all(request_line.as_bytes())
        .await
        .map_err(Error::Transport)?;

    write_half
        .write_all(b"Host: localhost\r\nAccept: application/json\r\nConnection: close\r\n")
        .await
        .map_err(Error::Transport)?;

    if let Some(etag) = if_match {
        write_half
            .write_all(format!("If-Match: {etag}\r\n").as_bytes())
            .await
            .map_err(Error::Transport)?;
    }

    if let Some(body) = body {
        write_half
            .write_all(
                format!(
                    "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .map_err(Error::Transport)?;
        write_half.write_all(body).await.map_err(Error::Transport)?;
    } else {
        write_half
            .write_all(b"\r\n")
            .await
            .map_err(Error::Transport)?;
    }
    write_half.flush().await.map_err(Error::Transport)?;

    read_response(&mut reader, max_response_bytes).await
}

fn build_request_line(method: Method, path: &str, query: &[(&str, &str)]) -> String {
    if query.is_empty() {
        format!("{} {} HTTP/1.1\r\n", method.as_str(), path)
    } else {
        let query_string = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(query)
            .finish();
        format!(
            "{} {}?{} HTTP/1.1\r\n",
            method.as_str(),
            path,
            query_string
        )
    }
}

async fn read_response<R>(reader: &mut BufReader<R>, max_bytes: usize) -> Result<RawResponse>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let status_line = read_line(reader, max_bytes).await?;
    let status = parse_status_line(&status_line)?;

    let mut headers = Vec::new();
    loop {
        let line = read_line(reader, max_bytes).await?;
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_owned(), value.trim().to_owned()));
        }
    }

    let content_length = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.parse::<usize>().ok());
    let is_chunked = headers.iter().any(|(name, value)| {
        name.eq_ignore_ascii_case("transfer-encoding") && value.eq_ignore_ascii_case("chunked")
    });

    let body = if let Some(length) = content_length {
        if length > max_bytes {
            return Err(Error::ResponseTooLarge { limit: max_bytes });
        }
        let mut buf = vec![0u8; length];
        reader
            .read_exact(&mut buf)
            .await
            .map_err(Error::Transport)?;
        buf
    } else if is_chunked {
        read_chunked_body(reader, max_bytes).await?
    } else {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            let n = reader.read(&mut chunk).await.map_err(Error::Transport)?;
            if n == 0 {
                break;
            }
            if buf.len() + n > max_bytes {
                return Err(Error::ResponseTooLarge { limit: max_bytes });
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        buf
    };

    Ok(RawResponse {
        status,
        headers,
        body,
    })
}

async fn read_chunked_body<R>(reader: &mut BufReader<R>, max_bytes: usize) -> Result<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut body = Vec::new();
    loop {
        let size_line = read_line(reader, max_bytes).await?;
        let size = usize::from_str_radix(size_line.trim(), 16).map_err(|_| {
            Error::InvalidResponse(format!("invalid chunk size line: {size_line:?}"))
        })?;
        if size == 0 {
            // Consume the trailing CRLF after the terminating 0-size chunk.
            let _ = read_line(reader, max_bytes).await?;
            break;
        }
        if body.len() + size > max_bytes {
            return Err(Error::ResponseTooLarge { limit: max_bytes });
        }
        let mut chunk = vec![0u8; size];
        reader
            .read_exact(&mut chunk)
            .await
            .map_err(Error::Transport)?;
        body.extend_from_slice(&chunk);
        // Each chunk is followed by a CRLF that isn't part of the payload.
        let _ = read_line(reader, max_bytes).await?;
    }
    Ok(body)
}

/// Reads one `\r\n`-terminated line (the `\r\n` stripped from the returned
/// string), enforcing `max_bytes` so a peer that never sends a newline can't
/// grow the buffer without bound.
async fn read_line<R>(reader: &mut BufReader<R>, max_bytes: usize) -> Result<String>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let n = reader.read(&mut byte).await.map_err(Error::Transport)?;
        if n == 0 {
            if buf.is_empty() {
                return Err(Error::Transport(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "connection closed before a complete response was received",
                )));
            }
            break;
        }
        if buf.len() + 1 > max_bytes {
            return Err(Error::ResponseTooLarge { limit: max_bytes });
        }
        if byte[0] == b'\n' {
            if buf.last() == Some(&b'\r') {
                buf.pop();
            }
            break;
        }
        buf.push(byte[0]);
    }
    String::from_utf8(buf)
        .map_err(|err| Error::InvalidResponse(format!("response line was not valid UTF-8: {err}")))
}

fn parse_status_line(line: &str) -> Result<u16> {
    let mut parts = line.split_whitespace();
    let _http_version = parts
        .next()
        .ok_or_else(|| Error::InvalidResponse(format!("empty status line: {line:?}")))?;
    let status = parts
        .next()
        .ok_or_else(|| Error::InvalidResponse(format!("malformed status line: {line:?}")))?;
    status
        .parse::<u16>()
        .map_err(|_| Error::InvalidResponse(format!("non-numeric status code: {status:?}")))
}

#[cfg(test)]
#[path = "unix_tests.rs"]
mod tests;
```

Note: the `read_line` implementation above reads byte-by-byte for simplicity and correctness (status/header lines are short, so this isn't a performance concern); it is intentionally simpler than `codex-app-server-client`'s `fill_buf`-based chunked reader since HTTP header lines don't need that optimization.

- [ ] **Step 5: Run the tests again**

Run: `cargo test -p incus-client transport::unix --all-features -- --nocapture`
Expected: all 5 tests in `unix_tests.rs` pass, including the concurrency test (fast request completes in well under 150ms while the slow request sleeps 300ms) and the disconnect/oversized-response tests (both complete promptly, not hanging until their 5s timeouts).

If the concurrency test is flaky under CI load, that's a real signal, not a test-tuning problem - re-verify the "fresh connection per request" design is actually being used and no shared lock was introduced.

- [ ] **Step 6: Write `transport.rs` — the `Client` type, envelope parsing, and the public `request()` entry point**

Create `crates/shared/incus-client/src/transport.rs` (replacing Task 1's stub):

```rust
//! The one place resource/operation code reaches the Incus daemon through:
//! [`Client::request`]. Everything below this module is HTTP framing
//! (`transport::unix`); everything above it (operations, resources) works
//! only with [`IncusEnvelope`], never raw bytes.

pub(crate) mod unix;

use std::path::PathBuf;
use std::sync::Arc;

use crate::config::ClientConfig;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }
}

/// A parsed Incus response envelope - see
/// <https://linuxcontainers.org/incus/docs/main/rest-api/>. Error responses
/// (HTTP 4xx/5xx, or a `{"type":"error",...}` body) are turned into
/// `Err(Error::Api { .. })` by [`Client::request`] rather than represented
/// here, so callers never need to check for an `Error` envelope variant
/// themselves.
#[derive(Debug, Clone)]
pub(crate) enum IncusEnvelope {
    Sync {
        metadata: serde_json::Value,
        etag: Option<String>,
    },
    Async {
        operation_url: String,
        /// The raw operation JSON object - `crate::operations` deserializes
        /// this into a typed `Operation`. Kept untyped here so this module
        /// has no dependency on `crate::operations`.
        metadata: serde_json::Value,
    },
}

/// A value paired with the ETag it was fetched with, for later use as an
/// `If-Match` precondition on an update.
#[derive(Debug, Clone)]
pub struct WithEtag<T> {
    pub value: T,
    pub etag: Option<String>,
}

#[derive(Debug, Clone)]
struct ClientInner {
    socket_path: PathBuf,
}

/// The Incus API client. Cheap to clone (`Arc`-backed) - share one instance
/// across tasks rather than constructing a new one per call.
#[derive(Debug, Clone)]
pub struct Client(Arc<ClientInner>);

impl Client {
    /// Builds a client from `config`. No I/O happens here - connection
    /// attempts happen lazily, once per request, when a method is called.
    #[must_use]
    pub fn new(config: ClientConfig) -> Self {
        Self(Arc::new(ClientInner {
            socket_path: config.socket_path,
        }))
    }

    /// Executes one Incus API request and returns its parsed envelope.
    /// Every resource and operation method in this crate is built on top of
    /// this one method - nothing else in the crate does its own HTTP
    /// framing or envelope parsing.
    pub(crate) async fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<&serde_json::Value>,
        if_match: Option<&str>,
    ) -> Result<IncusEnvelope> {
        let body_bytes = body.map(serde_json::to_vec).transpose()?;
        let raw = unix::execute(
            &self.0.socket_path,
            method,
            path,
            query,
            body_bytes.as_deref(),
            if_match,
        )
        .await?;

        if raw.status >= 400 {
            let error_body: serde_json::Value = serde_json::from_slice(&raw.body)
                .unwrap_or_else(|_| serde_json::json!({"error": "unparseable error body"}));
            let message = error_body
                .get("error")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown error")
                .to_owned();
            return Err(Error::Api {
                status_code: raw.status,
                message,
            });
        }

        let parsed: serde_json::Value = serde_json::from_slice(&raw.body)?;
        let envelope_type = parsed
            .get("type")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                Error::InvalidResponse(format!(
                    "response body had no \"type\" field: {parsed}"
                ))
            })?;

        match envelope_type {
            "sync" => {
                let metadata = parsed.get("metadata").cloned().unwrap_or(serde_json::Value::Null);
                let etag = raw.header("etag").map(str::to_owned);
                Ok(IncusEnvelope::Sync { metadata, etag })
            }
            "async" => {
                let operation_url = parsed
                    .get("operation")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        Error::InvalidResponse(
                            "async response had no \"operation\" field".to_owned(),
                        )
                    })?
                    .to_owned();
                let metadata = parsed
                    .get("metadata")
                    .cloned()
                    .ok_or_else(|| {
                        Error::InvalidResponse(
                            "async response had no \"metadata\" field".to_owned(),
                        )
                    })?;
                Ok(IncusEnvelope::Async {
                    operation_url,
                    metadata,
                })
            }
            other => Err(Error::InvalidResponse(format!(
                "unknown envelope type {other:?}"
            ))),
        }
    }
}
```

- [ ] **Step 7: Write `transport.rs`'s own tests**

Create `crates/shared/incus-client/src/transport_tests.rs`:

```rust
use super::*;

#[tokio::test]
async fn request_parses_a_sync_envelope() {
    let body = r#"{"type":"sync","status":"Success","status_code":200,"metadata":{"name":"c1"}}"#;
    let (socket_path, _dir) = crate::transport::unix::tests::spawn_fake_daemon(move |_req| {
        crate::transport::unix::tests::json_response("HTTP/1.1 200 OK", body)
    })
    .await;

    let client = Client::new(ClientConfig::unix_socket(socket_path));
    let envelope = client
        .request(Method::Get, "/1.0/instances/c1", &[], None, None)
        .await
        .expect("request should succeed");

    match envelope {
        IncusEnvelope::Sync { metadata, .. } => assert_eq!(metadata["name"], "c1"),
        other => panic!("expected Sync envelope, got {other:?}"),
    }
}

#[tokio::test]
async fn request_parses_an_async_envelope() {
    let body = r#"{"type":"async","status":"Operation created","status_code":100,"operation":"/1.0/operations/11111111-1111-1111-1111-111111111111","metadata":{"id":"11111111-1111-1111-1111-111111111111","class":"task","status":"Running","status_code":103,"resources":{},"may_cancel":false,"err":null}}"#;
    let (socket_path, _dir) = crate::transport::unix::tests::spawn_fake_daemon(move |_req| {
        crate::transport::unix::tests::json_response("HTTP/1.1 202 Accepted", body)
    })
    .await;

    let client = Client::new(ClientConfig::unix_socket(socket_path));
    let envelope = client
        .request(Method::Post, "/1.0/instances", &[], None, None)
        .await
        .expect("request should succeed");

    match envelope {
        IncusEnvelope::Async { operation_url, metadata } => {
            assert!(operation_url.contains("11111111"));
            assert_eq!(metadata["class"], "task");
        }
        other => panic!("expected Async envelope, got {other:?}"),
    }
}

#[tokio::test]
async fn request_maps_a_4xx_status_to_error_api() {
    let body = r#"{"type":"error","error":"not found","error_code":404}"#;
    let (socket_path, _dir) = crate::transport::unix::tests::spawn_fake_daemon(move |_req| {
        crate::transport::unix::tests::json_response("HTTP/1.1 404 Not Found", body)
    })
    .await;

    let client = Client::new(ClientConfig::unix_socket(socket_path));
    let err = client
        .request(Method::Get, "/1.0/instances/missing", &[], None, None)
        .await
        .expect_err("a 404 status must surface as an error");

    match err {
        crate::Error::Api { status_code, message } => {
            assert_eq!(status_code, 404);
            assert_eq!(message, "not found");
        }
        other => panic!("expected Error::Api, got {other:?}"),
    }
}
```

This test file reuses `spawn_fake_daemon`/`json_response` from `unix_tests.rs`. Make those two helpers `pub(crate)` (not private) in `unix_tests.rs` so `transport_tests.rs` can call them, and expose the `tests` module itself as `pub(crate)` in `unix.rs`'s wiring line:

Edit the bottom of `crates/shared/incus-client/src/transport/unix.rs`, change:

```rust
#[cfg(test)]
#[path = "unix_tests.rs"]
mod tests;
```

to:

```rust
#[cfg(test)]
#[path = "unix_tests.rs"]
pub(crate) mod tests;
```

And in `unix_tests.rs`, change `async fn spawn_fake_daemon` to `pub(crate) async fn spawn_fake_daemon` and `fn json_response` to `pub(crate) fn json_response`.

Wire `transport_tests.rs` at the bottom of `transport.rs`:

```rust
#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
```

- [ ] **Step 8: Re-add the `Client` re-export in `lib.rs`**

Edit `crates/shared/incus-client/src/lib.rs`, change:

```rust
pub use config::ClientConfig;
pub use error::{Error, Result};
```

to:

```rust
pub use config::ClientConfig;
pub use error::{Error, Result};
pub use transport::{Client, WithEtag};
```

(`WithEtag` is re-exported at the crate root now, not just reachable via `incus_client::transport::WithEtag`, since `Task::get_instance` in Task 5 returns it publicly and it should be as easy to reach as `Client` itself.)

- [ ] **Step 9: Run the full test suite for this task**

Run: `cargo test -p incus-client --all-features`
Expected: all tests pass (Task 1's 7 + Task 2's 5 unix tests + Task 2's 3 transport tests = 15 total).

Run: `cargo clippy -p incus-client --all-features -- -D warnings`
Expected: no warnings.

Run: `cargo tree -p incus-client -e normal`
Expected: no `reqwest` or `rustls` entries anywhere in the tree (confirms the mTLS deferral is honored at the dependency-graph level).

- [ ] **Step 10: Commit**

```bash
git add crates/shared/incus-client
git commit -m "feat(incus-client): unix-socket transport with envelope parsing"
```

---

## Task 3: Operations (wait/cancel lifecycle)

**Files:**
- Modify: `crates/shared/incus-client/src/error.rs` (already has `NotCancellable`/`OperationFailed` from Task 1 — no change needed unless a step below finds a gap)
- Modify: `crates/shared/incus-client/src/lib.rs` (add `pub mod operations;` — already present as a stub from Task 1, no edit needed here since Step 3 of Task 1 already added the `pub mod operations;` line)
- Create: `crates/shared/incus-client/src/operations.rs` (replaces Task 1's stub)
- Create: `crates/shared/incus-client/src/operations_tests.rs`

**Interfaces:**
- Consumes: `crate::transport::{Client, Method, IncusEnvelope}` (Task 2), `crate::error::{Error, Result}` (Task 1).
- Produces: `pub struct Operation { pub id: uuid::Uuid, pub class: OperationClass, pub status: String, pub status_code: u16, pub resources: serde_json::Value, pub metadata: Option<serde_json::Value>, pub may_cancel: bool, pub err: Option<String> }`, `pub enum OperationClass { Task, Websocket, Token }`, `impl Client { pub async fn wait_for_operation(&self, id: uuid::Uuid, timeout: Option<Duration>) -> Result<Operation>; pub async fn cancel_operation(&self, id: uuid::Uuid) -> Result<()>; }`, `pub(crate) fn operation_from_envelope(envelope: IncusEnvelope) -> Result<Operation>` (used by every resource-mutation method in Tasks 5-6).

### Design notes

`wait_for_operation`'s `timeout` parameter maps directly to Incus's `.../wait?timeout=<seconds>` query parameter (a server-side bound on one long-poll call), not a client-side wrapper. If that single long-poll window elapses before the operation reaches a terminal status and the caller passed `None`, this method transparently re-issues the wait call (looping) rather than returning a premature in-progress `Operation` — so `None` means "wait indefinitely via repeated long-polls," not "no timeout at all in a single call." If the caller passed `Some(duration)` explicitly, one elapsed window returns the in-progress `Operation` as `Ok(..)` without re-issuing, honoring the caller's explicit bound.

- [ ] **Step 1: Write the failing tests**

Create `crates/shared/incus-client/src/operations_tests.rs`:

```rust
use super::*;
use crate::config::ClientConfig;
use crate::transport::Client;

fn success_operation_json(id: &str, status_code: u16) -> String {
    format!(
        r#"{{"type":"sync","status":"Success","status_code":200,"metadata":{{"id":"{id}","class":"task","status":"Success","status_code":{status_code},"resources":{{}},"may_cancel":false,"err":null}}}}"#
    )
}

fn failure_operation_json(id: &str) -> String {
    format!(
        r#"{{"type":"sync","status":"Success","status_code":200,"metadata":{{"id":"{id}","class":"task","status":"Failure","status_code":400,"resources":{{}},"may_cancel":false,"err":"storage pool full"}}}}"#
    )
}

fn in_progress_operation_json(id: &str) -> String {
    format!(
        r#"{{"type":"sync","status":"Success","status_code":200,"metadata":{{"id":"{id}","class":"task","status":"Running","status_code":103,"resources":{{}},"may_cancel":true,"err":null}}}}"#
    )
}

#[tokio::test]
async fn wait_for_operation_returns_the_operation_on_success() {
    let id = uuid::Uuid::new_v4();
    let body = success_operation_json(&id.to_string(), 200);
    let (socket_path, _dir) = crate::transport::unix::tests::spawn_fake_daemon(move |_req| {
        crate::transport::unix::tests::json_response("HTTP/1.1 200 OK", &body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let op = client
        .wait_for_operation(id, Some(std::time::Duration::from_secs(1)))
        .await
        .expect("success operation should be returned, not an error");

    assert_eq!(op.status_code, 200);
    assert_eq!(op.id, id);
}

#[tokio::test]
async fn wait_for_operation_returns_operation_failed_on_failure_status() {
    let id = uuid::Uuid::new_v4();
    let body = failure_operation_json(&id.to_string());
    let (socket_path, _dir) = crate::transport::unix::tests::spawn_fake_daemon(move |_req| {
        crate::transport::unix::tests::json_response("HTTP/1.1 200 OK", &body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let err = client
        .wait_for_operation(id, Some(std::time::Duration::from_secs(1)))
        .await
        .expect_err("a failure-range status_code must surface as Err, not Ok");

    match err {
        crate::Error::OperationFailed { id: err_id, status_code, err: message } => {
            assert_eq!(err_id, id);
            assert_eq!(status_code, 400);
            assert_eq!(message.as_deref(), Some("storage pool full"));
        }
        other => panic!("expected Error::OperationFailed, got {other:?}"),
    }
}

#[tokio::test]
async fn wait_for_operation_with_explicit_timeout_returns_in_progress_without_repolling() {
    let id = uuid::Uuid::new_v4();
    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = call_count.clone();
    let body = in_progress_operation_json(&id.to_string());
    let (socket_path, _dir) = crate::transport::unix::tests::spawn_fake_daemon(move |_req| {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        crate::transport::unix::tests::json_response("HTTP/1.1 200 OK", &body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let op = client
        .wait_for_operation(id, Some(std::time::Duration::from_millis(50)))
        .await
        .expect("in-progress with an explicit timeout should return Ok, not error or hang");

    assert_eq!(op.status_code, 103);
    assert_eq!(
        call_count.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "an explicit timeout must not trigger automatic re-polling"
    );
}

#[tokio::test]
async fn wait_for_operation_with_none_timeout_repolls_until_terminal_status() {
    let id = uuid::Uuid::new_v4();
    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = call_count.clone();
    let id_string = id.to_string();
    let (socket_path, _dir) = crate::transport::unix::tests::spawn_fake_daemon(move |_req| {
        let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let body = if n == 0 {
            in_progress_operation_json(&id_string)
        } else {
            success_operation_json(&id_string, 200)
        };
        crate::transport::unix::tests::json_response("HTTP/1.1 200 OK", &body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let op = client
        .wait_for_operation(id, None)
        .await
        .expect("None timeout should re-poll past the in-progress response to success");

    assert_eq!(op.status_code, 200);
    assert!(
        call_count.load(std::sync::atomic::Ordering::SeqCst) >= 2,
        "expected at least one re-poll after the first in-progress response"
    );
}

#[tokio::test]
async fn cancel_operation_short_circuits_without_a_network_call_when_not_cancellable() {
    // Bind a listener but never accept a connection from it - if
    // cancel_operation made a network call here, the test would hang until
    // its own timeout rather than returning immediately.
    let dir = tempfile::tempdir().expect("create temp dir");
    let socket_path = dir.path().join("incus.sock");
    let _listener = tokio::net::UnixListener::bind(&socket_path).expect("bind");

    let client = Client::new(ClientConfig::unix_socket(socket_path));
    let id = uuid::Uuid::new_v4();

    // cancel_operation needs an Operation snapshot to check may_cancel
    // against - it takes the Operation directly rather than re-fetching by
    // id, so no network call is possible for the not-cancellable case.
    let op = Operation {
        id,
        class: OperationClass::Task,
        status: "Running".to_owned(),
        status_code: 103,
        resources: serde_json::json!({}),
        metadata: None,
        may_cancel: false,
        err: None,
    };

    let result = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        client.cancel_operation(&op),
    )
    .await
    .expect("must return immediately, not hang waiting on a network call");

    assert!(matches!(result, Err(crate::Error::NotCancellable)));
}

#[test]
fn operation_class_serde_round_trips_lowercase_wire_values() {
    for (variant, wire) in [
        (OperationClass::Task, "\"task\""),
        (OperationClass::Websocket, "\"websocket\""),
        (OperationClass::Token, "\"token\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, wire);
        let deserialized: OperationClass = serde_json::from_str(wire).unwrap();
        assert_eq!(deserialized, variant);
    }
}

#[test]
fn operation_deserializes_from_a_real_example_payload() {
    // Copied verbatim from the Incus REST API docs' operation object shape
    // (https://linuxcontainers.org/incus/docs/main/rest-api/), so this test
    // exercises the actual documented wire format, not a hand-constructed
    // assumption about it.
    let json = r#"{
        "id": "6916ee11-cf7f-4dd9-861f-e2ba7f4e2ea3",
        "class": "task",
        "status": "Running",
        "status_code": 103,
        "resources": {"containers": ["/1.0/containers/test"]},
        "metadata": null,
        "may_cancel": false,
        "err": ""
    }"#;
    let op: Operation = serde_json::from_str(json).expect("must deserialize");
    assert_eq!(op.class, OperationClass::Task);
    assert_eq!(op.status_code, 103);
    assert!(!op.may_cancel);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p incus-client operations --all-features 2>&1 | head -40`
Expected: compile failure — `Operation`, `OperationClass`, `wait_for_operation`, `cancel_operation` don't exist yet.

- [ ] **Step 3: Implement `operations.rs`**

Create `crates/shared/incus-client/src/operations.rs` (replacing Task 1's stub):

```rust
//! Async-operation lifecycle: every mutation Incus documents as
//! long-running returns one of these, which callers wait on via
//! [`Client::wait_for_operation`] rather than assuming synchronous
//! completion.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::transport::{Client, IncusEnvelope, Method};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationClass {
    Task,
    Websocket,
    Token,
}

/// An Incus asynchronous operation - see
/// <https://linuxcontainers.org/incus/docs/main/rest-api/>. `resources` and
/// `metadata` stay untyped (`serde_json::Value`) because their shape varies
/// per operation kind; the well-known top-level fields are fully typed.
#[derive(Debug, Clone, Deserialize)]
pub struct Operation {
    pub id: Uuid,
    pub class: OperationClass,
    pub status: String,
    pub status_code: u16,
    #[serde(default)]
    pub resources: serde_json::Value,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    pub may_cancel: bool,
    #[serde(default)]
    pub err: Option<String>,
}

impl Operation {
    fn is_terminal(&self) -> bool {
        // Per Incus's 3-digit status_code scheme: 100-199 are in-progress
        // states, 200-399 are positive (terminal) results, 400-599 are
        // negative (terminal) results.
        self.status_code >= 200
    }

    fn is_failure(&self) -> bool {
        self.status_code >= 400
    }
}

/// Converts an [`IncusEnvelope::Async`]'s untyped `metadata` into a typed
/// [`Operation`]. Also accepts [`IncusEnvelope::Sync`] (used by
/// `wait_for_operation`, whose `.../wait` endpoint returns the operation
/// object directly as sync metadata, not wrapped in another async envelope).
pub(crate) fn operation_from_envelope(envelope: IncusEnvelope) -> Result<Operation> {
    let metadata = match envelope {
        IncusEnvelope::Async { metadata, .. } => metadata,
        IncusEnvelope::Sync { metadata, .. } => metadata,
    };
    Ok(serde_json::from_value(metadata)?)
}

impl Client {
    /// Waits for operation `id` to reach a terminal status, using Incus's
    /// `.../wait?timeout=<seconds>` long-poll endpoint.
    ///
    /// - `timeout = Some(duration)`: bounds a *single* long-poll call. If
    ///   the operation is still in-progress when that window elapses, this
    ///   returns `Ok(Operation)` with the in-progress snapshot - it does
    ///   **not** re-poll, since the caller explicitly chose how long they're
    ///   willing to wait.
    /// - `timeout = None`: waits indefinitely by transparently re-issuing
    ///   the long-poll call as many times as needed until a terminal status
    ///   is reached. Each individual call is still bounded server-side; this
    ///   just means the method as a whole doesn't return until completion.
    ///
    /// A terminal status in the 400-599 (failure) range returns
    /// `Err(Error::OperationFailed { .. })`, not `Ok(Operation)` - callers
    /// don't need to inspect `status_code` themselves to detect failure.
    pub async fn wait_for_operation(&self, id: Uuid, timeout: Option<Duration>) -> Result<Operation> {
        loop {
            let query_value;
            let query: &[(&str, &str)] = if let Some(duration) = timeout {
                query_value = duration.as_secs().to_string();
                &[("timeout", query_value.as_str())]
            } else {
                &[]
            };
            let path = format!("/1.0/operations/{id}/wait");
            let envelope = self.request(Method::Get, &path, query, None, None).await?;
            let operation = operation_from_envelope(envelope)?;

            if !operation.is_terminal() {
                if timeout.is_some() {
                    // Caller set an explicit bound on one long-poll call;
                    // honor it rather than looping past it.
                    return Ok(operation);
                }
                // No caller-set bound: this window elapsed without a
                // terminal status, so re-issue the wait call and keep
                // waiting.
                continue;
            }

            if operation.is_failure() {
                return Err(Error::OperationFailed {
                    id: operation.id,
                    status_code: operation.status_code,
                    err: operation.err,
                });
            }

            return Ok(operation);
        }
    }

    /// Cancels operation `op` if it's cancellable. Short-circuits with
    /// `Error::NotCancellable` (no network call) when `op.may_cancel` is
    /// false, since the server would reject it anyway and there's no reason
    /// to round-trip to find that out.
    pub async fn cancel_operation(&self, op: &Operation) -> Result<()> {
        if !op.may_cancel {
            return Err(Error::NotCancellable);
        }
        let path = format!("/1.0/operations/{}", op.id);
        self.request(Method::Delete, &path, &[], None, None).await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "operations_tests.rs"]
mod tests;
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p incus-client operations --all-features -- --nocapture`
Expected: all 8 tests pass.

Run: `cargo clippy -p incus-client --all-features -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/shared/incus-client
git commit -m "feat(incus-client): operation wait/cancel lifecycle"
```

---

## Task 4: Events (WebSocket) + Resources Module Skeleton

**Files:**
- Create: `crates/shared/incus-client/src/events.rs`
- Create: `crates/shared/incus-client/src/events_tests.rs`
- Create: `crates/shared/incus-client/src/resources.rs` (replaces Task 1's stub)
- Create: `crates/shared/incus-client/src/resources/instances.rs` (stub — Task 5 implements)
- Create: `crates/shared/incus-client/src/resources/images.rs` (stub — Task 6 implements)
- Create: `crates/shared/incus-client/src/resources/networks.rs` (stub — Task 6 implements)
- Create: `crates/shared/incus-client/src/resources/storage.rs` (stub — Task 6 implements)
- Create: `crates/shared/incus-client/src/resources/projects.rs` (stub — Task 6 implements)

**Interfaces:**
- Consumes: `crate::transport::Client` (Task 2), `crate::operations::Operation` (Task 3).
- Produces (behind `events` feature): `pub enum Event { Operation(Operation), Lifecycle(serde_json::Value), Logging(serde_json::Value) }`, `pub struct EventFilter { pub operations: bool, pub lifecycle: bool, pub logging: bool }`, `impl Client { pub async fn subscribe_events(&self, filter: EventFilter) -> Result<EventStream>; }` where `EventStream` directly re-exposes the underlying WebSocket stream (no intermediate buffering task, per the locked no-unbounded-channel requirement).

This crate's unix-socket transport doesn't reach `/1.0/events` over plain WebSocket-over-TCP the way `tokio-tungstenite`'s `connect_async` normally would (that assumes a TCP/TLS stream) — Incus's events endpoint is reached the same way every other endpoint is, over the Unix socket, with an `Upgrade: websocket` handshake. `tokio-tungstenite` supports building a client handshake over *any* `AsyncRead + AsyncWrite` stream via `tokio_tungstenite::client_async`, which is what makes this work over a `UnixStream` instead of a TCP stream.

- [ ] **Step 1: Write the failing test**

Create `crates/shared/incus-client/src/events_tests.rs`:

```rust
use futures::StreamExt;
use tokio::net::UnixListener;
use tokio_tungstenite::tungstenite::Message;

use super::*;
use crate::config::ClientConfig;
use crate::transport::Client;

/// Spawns a fake Incus daemon that accepts one WebSocket connection on a
/// Unix socket and sends the given canned text frames before closing.
async fn spawn_fake_events_daemon(frames: Vec<String>) -> (std::path::PathBuf, tempfile::TempDir) {
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
        let _ = ws.close(None).await;
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

#[test]
fn event_filter_default_subscribes_to_everything() {
    let filter = EventFilter::default();
    assert!(filter.operations);
    assert!(filter.lifecycle);
    assert!(filter.logging);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p incus-client events --all-features 2>&1 | head -40`
Expected: compile failure — `Event`, `EventFilter`, `subscribe_events` don't exist yet. Also add `futures = "0.3"` as a dev-dependency (needed for `StreamExt` in the test) — edit `crates/shared/incus-client/Cargo.toml`'s `[dev-dependencies]`:

```toml
futures = "0.3"
```

- [ ] **Step 3: Implement `events.rs`**

Create `crates/shared/incus-client/src/events.rs`:

```rust
//! WebSocket subscription to Incus's `/1.0/events` push-notification
//! stream. This is an *enhancement* over [`crate::operations::Client::wait_for_operation`],
//! not a replacement - that method works without this `events` feature at
//! all.
//!
//! `subscribe_events` directly re-exposes the underlying WebSocket stream
//! rather than buffering through an intermediate channel, so a slow
//! consumer simply leaves frames unread in the transport's own receive
//! buffer (natural backpressure) instead of risking unbounded in-process
//! buffering.

use futures::{Stream, StreamExt};
use serde::Deserialize;
use tokio::net::UnixStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::error::{Error, Result};
use crate::operations::Operation;
use crate::transport::Client;

/// Which event types to subscribe to. All `true` by default (subscribe to
/// everything Incus emits).
#[derive(Debug, Clone, Copy)]
pub struct EventFilter {
    pub operations: bool,
    pub lifecycle: bool,
    pub logging: bool,
}

impl Default for EventFilter {
    fn default() -> Self {
        Self {
            operations: true,
            lifecycle: true,
            logging: true,
        }
    }
}

impl EventFilter {
    fn query_value(self) -> String {
        let mut types = Vec::new();
        if self.operations {
            types.push("operation");
        }
        if self.lifecycle {
            types.push("lifecycle");
        }
        if self.logging {
            types.push("logging");
        }
        types.join(",")
    }
}

/// One event from the `/1.0/events` stream. `Lifecycle` and `Logging`
/// payloads stay untyped (`serde_json::Value`) for v1 - only `Operation`
/// events are fully typed, since that's what operation-completion tracking
/// needs.
#[derive(Debug, Clone)]
pub enum Event {
    Operation(Operation),
    Lifecycle(serde_json::Value),
    Logging(serde_json::Value),
}

#[derive(Debug, Deserialize)]
struct RawFrame {
    #[serde(rename = "type")]
    kind: String,
    metadata: serde_json::Value,
}

fn parse_event(text: &str) -> Result<Event> {
    let frame: RawFrame = serde_json::from_str(text)?;
    match frame.kind.as_str() {
        "operation" => Ok(Event::Operation(serde_json::from_value(frame.metadata)?)),
        "lifecycle" => Ok(Event::Lifecycle(frame.metadata)),
        "logging" => Ok(Event::Logging(frame.metadata)),
        other => Err(Error::InvalidResponse(format!(
            "unknown event frame type {other:?}"
        ))),
    }
}

/// The event stream returned by [`Client::subscribe_events`]. Yields
/// `Result<Event>` - a malformed frame surfaces as one `Err` item rather
/// than terminating the whole stream, since one bad frame from a busy
/// daemon shouldn't take down an otherwise-healthy subscription.
pub struct EventStream {
    inner: WebSocketStream<MaybeTlsStream<UnixStream>>,
}

impl Stream for EventStream {
    type Item = Result<Event>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        loop {
            match self.inner.poll_next_unpin(cx) {
                std::task::Poll::Ready(Some(Ok(Message::Text(text)))) => {
                    return std::task::Poll::Ready(Some(parse_event(text.as_str())));
                }
                std::task::Poll::Ready(Some(Ok(Message::Ping(_) | Message::Pong(_)))) => {
                    // tokio-tungstenite answers pings automatically; nothing
                    // for the caller to see here - poll again.
                    continue;
                }
                std::task::Poll::Ready(Some(Ok(Message::Close(_)))) | std::task::Poll::Ready(None) => {
                    return std::task::Poll::Ready(None);
                }
                std::task::Poll::Ready(Some(Ok(Message::Binary(_) | Message::Frame(_)))) => {
                    return std::task::Poll::Ready(Some(Err(Error::InvalidResponse(
                        "unexpected binary websocket frame from /1.0/events".to_owned(),
                    ))));
                }
                std::task::Poll::Ready(Some(Err(err))) => {
                    return std::task::Poll::Ready(Some(Err(Error::Transport(
                        std::io::Error::other(err.to_string()),
                    ))));
                }
                std::task::Poll::Pending => return std::task::Poll::Pending,
            }
        }
    }
}

impl Client {
    /// Subscribes to Incus's `/1.0/events` WebSocket stream, filtered per
    /// `filter`. The connection is made over the same Unix socket as every
    /// other request - see the module doc comment for why the resulting
    /// stream is exposed directly rather than through a buffering channel.
    pub async fn subscribe_events(&self, filter: EventFilter) -> Result<EventStream> {
        let socket_path = self.socket_path();
        let stream = UnixStream::connect(&socket_path)
            .await
            .map_err(Error::Transport)?;
        let request = format!(
            "ws://localhost/1.0/events?type={}",
            filter.query_value()
        );
        let (ws_stream, _response) = tokio_tungstenite::client_async(request, stream)
            .await
            .map_err(|err| Error::Transport(std::io::Error::other(err.to_string())))?;
        Ok(EventStream {
            inner: WebSocketStream::from_raw_socket(
                MaybeTlsStream::Plain(ws_stream.get_ref().get_ref().clone()),
                ws_stream.get_config(),
                None,
            )
            .await,
        })
    }
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
```

**Note for the implementing engineer:** the `subscribe_events` body above composes `tokio_tungstenite::client_async` (which returns a `WebSocketStream<UnixStream>` directly, not wrapped in `MaybeTlsStream`) with an unnecessary re-wrap through `MaybeTlsStream` — that re-wrap doesn't compile as written (`ws_stream.get_ref().get_ref()` doesn't produce a cloneable stream, and `client_async`'s return type is already the right `WebSocketStream<UnixStream>`). Fix this when implementing: change `EventStream.inner`'s type to `WebSocketStream<UnixStream>` (drop `MaybeTlsStream` entirely — this crate has no TLS transport in this epic) and simplify `subscribe_events` to:

```rust
pub async fn subscribe_events(&self, filter: EventFilter) -> Result<EventStream> {
    let socket_path = self.socket_path();
    let stream = UnixStream::connect(&socket_path)
        .await
        .map_err(Error::Transport)?;
    let request = format!("ws://localhost/1.0/events?type={}", filter.query_value());
    let (ws_stream, _response) = tokio_tungstenite::client_async(request, stream)
        .await
        .map_err(|err| Error::Transport(std::io::Error::other(err.to_string())))?;
    Ok(EventStream { inner: ws_stream })
}
```

and `EventStream { inner: WebSocketStream<UnixStream> }`. This requires `Client` to expose its socket path to `events.rs` — add a small `pub(crate) fn socket_path(&self) -> &std::path::Path` method to `impl Client` in `transport.rs` (Task 2's file) as part of this task's diff, since Task 2 didn't need it and this is the first caller.

- [ ] **Step 4: Add the `socket_path()` accessor to `Client`**

Edit `crates/shared/incus-client/src/transport.rs`, inside `impl Client`, add:

```rust
pub(crate) fn socket_path(&self) -> &std::path::Path {
    &self.0.socket_path
}
```

- [ ] **Step 5: Add the `futures` dependency (non-dev, since `events.rs` uses `Stream`/`StreamExt` in production code, not just tests)**

Edit `crates/shared/incus-client/Cargo.toml`, add to `[dependencies]`:

```toml
futures = "0.3"
```

(Remove the dev-dependency `futures = "0.3"` added in Step 2 if it was added there — one entry in `[dependencies]` covers both production and test code.)

- [ ] **Step 6: Run the tests**

Run: `cargo test -p incus-client events --all-features -- --nocapture`
Expected: all 3 tests pass.

Run: `cargo test -p incus-client` (no `--all-features`)
Expected: passes, and `events.rs` is not compiled at all (confirm with `cargo build -p incus-client 2>&1` showing no references to `tokio_tungstenite`).

- [ ] **Step 7: Write the resources module skeleton**

Create `crates/shared/incus-client/src/resources.rs` (replacing Task 1's stub):

```rust
//! Resource CRUD surfaces. Each submodule owns one Incus resource type and
//! is implemented independently against the stub files created here -
//! `instances` in one pass, `images`/`networks`/`storage`/`projects`
//! together in another, since they don't share any files.

pub mod images;
pub mod instances;
pub mod networks;
pub mod projects;
pub mod storage;
```

Create the five stub files (each implemented by a later task — leave them as doc-comment-only stubs for now so the crate compiles):

`crates/shared/incus-client/src/resources/instances.rs`:

```rust
//! Instance (container/VM) CRUD, lifecycle, and snapshots. Implemented by
//! the "instances resource" task. Deliberately excludes exec, console
//! attach, and file push/pull - those use a different WebSocket-secrets
//! protocol than this crate's generic operations/events model and are
//! out of scope for this epic.
```

`crates/shared/incus-client/src/resources/images.rs`:

```rust
//! Image CRUD. Implemented alongside networks/storage/projects.
```

`crates/shared/incus-client/src/resources/networks.rs`:

```rust
//! Network CRUD. Implemented alongside images/storage/projects.
```

`crates/shared/incus-client/src/resources/storage.rs`:

```rust
//! Storage pool and volume CRUD (pool -> volume nesting). Implemented
//! alongside images/networks/projects.
```

`crates/shared/incus-client/src/resources/projects.rs`:

```rust
//! Project CRUD. Implemented alongside images/networks/storage.
```

- [ ] **Step 8: Build and test the whole crate**

Run: `cargo build -p incus-client --all-features`
Expected: builds cleanly.

Run: `cargo test -p incus-client --all-features`
Expected: all tests from Tasks 1-4 pass (15 + 8 + 3 = 26 total, plus the 3 transport tests already counted in Task 2 = confirm total via `cargo test -p incus-client --all-features 2>&1 | tail -5`).

Run: `cargo clippy -p incus-client --all-features -- -D warnings`
Expected: no warnings.

- [ ] **Step 9: Commit**

```bash
git add crates/shared/incus-client
git commit -m "feat(incus-client): events subscription + resources module skeleton"
```

---

## Task 5: Instances Resource

**Files:**
- Modify: `crates/shared/incus-client/src/resources/instances.rs` (implement — replaces Task 4's stub)
- Create: `crates/shared/incus-client/src/resources/instances_tests.rs`

**Interfaces:**
- Consumes: `crate::transport::{Client, Method, WithEtag}` (Task 2), `crate::operations::{Operation, operation_from_envelope}` (Task 3).
- Produces: `pub struct Instance { .. }`, `pub struct CreateInstanceParams { .. }`, `impl Client { list_instances, get_instance, create_instance, update_instance, patch_instance, delete_instance, start_instance, stop_instance, restart_instance, pause_instance, list_snapshots, create_snapshot, delete_snapshot }`.

**Can run in parallel with Task 6** — disjoint files.

- [ ] **Step 1: Write the failing tests**

Create `crates/shared/incus-client/src/resources/instances_tests.rs`:

```rust
use super::*;
use crate::config::ClientConfig;
use crate::transport::{Client, unix::tests::{json_response, spawn_fake_daemon}};

fn instance_json(name: &str) -> String {
    format!(
        r#"{{"type":"sync","status":"Success","status_code":200,"metadata":{{"name":"{name}","status":"Running","status_code":103,"type":"container","architecture":"x86_64","created_at":"2026-01-01T00:00:00Z","last_used_at":"2026-01-01T00:00:00Z","location":"none","project":"default","config":{{}},"devices":{{}},"profiles":["default"]}}}}"#
    )
}

fn operation_json(id: &str) -> String {
    format!(
        r#"{{"type":"async","status":"Operation created","status_code":100,"operation":"/1.0/operations/{id}","metadata":{{"id":"{id}","class":"task","status":"Running","status_code":103,"resources":{{}},"may_cancel":true,"err":null}}}}"#
    )
}

#[tokio::test]
async fn get_instance_deserializes_the_documented_shape_and_returns_etag() {
    let body = instance_json("c1");
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"abc123\"\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        response.into_bytes()
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let WithEtag { value: instance, etag } = client
        .get_instance("c1")
        .await
        .expect("get_instance should succeed");

    assert_eq!(instance.name, "c1");
    assert_eq!(instance.instance_type, "container");
    assert_eq!(etag.as_deref(), Some("\"abc123\""));
}

#[tokio::test]
async fn list_instances_passes_recursion_through_as_a_query_param() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let body = r#"{"type":"sync","status":"Success","status_code":200,"metadata":[]}"#;
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        *seen.lock().unwrap() = String::from_utf8_lossy(&req).into_owned();
        json_response("HTTP/1.1 200 OK", body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    client
        .list_instances(true)
        .await
        .expect("list_instances should succeed");

    assert!(seen_request.lock().unwrap().contains("recursion=true"));
}

#[tokio::test]
async fn create_instance_returns_an_operation() {
    let id = uuid::Uuid::new_v4().to_string();
    let body = operation_json(&id);
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| {
        json_response("HTTP/1.1 202 Accepted", &body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let params = CreateInstanceParams {
        name: "c1".to_owned(),
        instance_type: "container".to_owned(),
        source: serde_json::json!({"type": "image", "fingerprint": "abc123"}),
    };
    let op: Operation = client
        .create_instance(&params)
        .await
        .expect("create_instance should return an Operation");

    assert_eq!(op.id.to_string(), id);
}

#[tokio::test]
async fn update_instance_sends_if_match_header_when_etag_is_provided() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let id = uuid::Uuid::new_v4().to_string();
    let body = operation_json(&id);
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        *seen.lock().unwrap() = String::from_utf8_lossy(&req).into_owned();
        json_response("HTTP/1.1 202 Accepted", &body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    client
        .update_instance("c1", &serde_json::json!({"config": {}}), Some("\"abc123\""))
        .await
        .expect("update_instance should succeed");

    assert!(seen_request.lock().unwrap().contains("If-Match: \"abc123\""));
}

#[tokio::test]
async fn update_instance_maps_412_to_precondition_failed() {
    let body = r#"{"type":"error","error":"stale etag","error_code":412}"#;
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| {
        json_response("HTTP/1.1 412 Precondition Failed", body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let err = client
        .update_instance("c1", &serde_json::json!({}), Some("\"stale\""))
        .await
        .expect_err("412 must map to a distinguishable error");

    assert!(matches!(
        err,
        crate::Error::PreconditionFailed { resource } if resource == "c1"
    ));
}

#[tokio::test]
async fn patch_instance_maps_412_to_precondition_failed() {
    let body = r#"{"type":"error","error":"stale etag","error_code":412}"#;
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| {
        json_response("HTTP/1.1 412 Precondition Failed", body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let err = client
        .patch_instance("c1", &serde_json::json!({"config": {}}), Some("\"stale\""))
        .await
        .expect_err("412 must map to a distinguishable error on PATCH too");

    assert!(matches!(err, crate::Error::PreconditionFailed { .. }));
}

#[tokio::test]
async fn delete_instance_returns_an_operation() {
    let id = uuid::Uuid::new_v4().to_string();
    let body = operation_json(&id);
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| json_response("HTTP/1.1 202 Accepted", &body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let op = client.delete_instance("c1").await.expect("delete should succeed");
    assert_eq!(op.id.to_string(), id);
}

#[tokio::test]
async fn state_transitions_return_operations() {
    let id = uuid::Uuid::new_v4().to_string();
    let body = operation_json(&id);
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| json_response("HTTP/1.1 202 Accepted", &body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    for op_result in [
        client.start_instance("c1").await,
        client.stop_instance("c1").await,
        client.restart_instance("c1").await,
        client.pause_instance("c1").await,
    ] {
        let op = op_result.expect("state transition should return an Operation");
        assert_eq!(op.id.to_string(), id);
    }
}

#[tokio::test]
async fn snapshot_create_and_delete_return_operations() {
    let id = uuid::Uuid::new_v4().to_string();
    let body = operation_json(&id);
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| json_response("HTTP/1.1 202 Accepted", &body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let create_op = client
        .create_snapshot("c1", "snap1")
        .await
        .expect("create_snapshot should return an Operation");
    assert_eq!(create_op.id.to_string(), id);

    let delete_op = client
        .delete_snapshot("c1", "snap1")
        .await
        .expect("delete_snapshot should return an Operation");
    assert_eq!(delete_op.id.to_string(), id);
}

#[tokio::test]
async fn list_snapshots_deserializes_a_list() {
    let body = r#"{"type":"sync","status":"Success","status_code":200,"metadata":["/1.0/instances/c1/snapshots/snap1"]}"#;
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| json_response("HTTP/1.1 200 OK", body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let snapshots = client
        .list_snapshots("c1", false)
        .await
        .expect("list_snapshots should succeed");
    assert_eq!(snapshots.len(), 1);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p incus-client resources::instances --all-features 2>&1 | head -60`
Expected: compile failure — none of these types/methods exist yet.

- [ ] **Step 3: Implement `resources/instances.rs`**

Replace the stub `crates/shared/incus-client/src/resources/instances.rs` with:

```rust
//! Instance (container/VM) CRUD, lifecycle, and snapshots.
//!
//! Exec, console attach, and file push/pull are deliberately **not**
//! implemented here: each uses `POST .../exec`-style operations whose
//! `metadata` carries secrets for separate control/stdin/stdout WebSocket
//! connections - a materially different protocol from the generic
//! operations/events model the rest of this crate is built on. That's
//! follow-up work for whenever a real consumer needs it, not a gap in this
//! epic.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::operations::{operation_from_envelope, Operation};
use crate::transport::{Client, Method, WithEtag};

/// A container or virtual machine. `config`/`devices` stay untyped
/// (`serde_json::Value`) - Incus's instance config schema is large and
/// mostly free-form key-value pairs, so fully typing it is out of scope for
/// this crate.
#[derive(Debug, Clone, Deserialize)]
pub struct Instance {
    pub name: String,
    pub status: String,
    pub status_code: u16,
    #[serde(rename = "type")]
    pub instance_type: String,
    pub architecture: String,
    pub created_at: String,
    pub last_used_at: String,
    pub location: String,
    pub project: String,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub devices: serde_json::Value,
    #[serde(default)]
    pub profiles: Vec<String>,
}

/// Parameters for [`Client::create_instance`]. `source` is the raw Incus
/// source-object JSON (e.g. `{"type": "image", "fingerprint": "..."}`) -
/// kept untyped since Incus supports several distinct source shapes
/// (image, copy, migration, none) that aren't worth fully typing for v1.
#[derive(Debug, Clone, Serialize)]
pub struct CreateInstanceParams {
    pub name: String,
    #[serde(rename = "type")]
    pub instance_type: String,
    pub source: serde_json::Value,
}

fn precondition_failed_or(err: Error, resource: &str) -> Error {
    match err {
        Error::Api { status_code: 412, .. } => Error::PreconditionFailed {
            resource: resource.to_owned(),
        },
        other => other,
    }
}

impl Client {
    /// Lists instances. `recursion = true` fetches every instance's full
    /// object (config/devices/state) in one call and can be expensive on
    /// hosts with many instances; `recursion = false` returns lightweight
    /// name/URL references only.
    pub async fn list_instances(&self, recursion: bool) -> Result<Vec<serde_json::Value>> {
        let recursion_value = recursion.to_string();
        let query = [("recursion", recursion_value.as_str())];
        let envelope = self
            .request(Method::Get, "/1.0/instances", &query, None, None)
            .await?;
        match envelope {
            crate::transport::IncusEnvelope::Sync { metadata, .. } => Ok(serde_json::from_value(metadata)?),
            other => Err(Error::InvalidResponse(format!(
                "expected a sync list response, got {other:?}"
            ))),
        }
    }

    /// Fetches one instance by name, along with its ETag for use as a later
    /// `If-Match` precondition.
    pub async fn get_instance(&self, name: &str) -> Result<WithEtag<Instance>> {
        let path = format!("/1.0/instances/{name}");
        let envelope = self.request(Method::Get, &path, &[], None, None).await?;
        match envelope {
            crate::transport::IncusEnvelope::Sync { metadata, etag } => Ok(WithEtag {
                value: serde_json::from_value(metadata)?,
                etag,
            }),
            other => Err(Error::InvalidResponse(format!(
                "expected a sync instance response, got {other:?}"
            ))),
        }
    }

    /// Creates an instance. Always async, per Incus's documented behavior
    /// for instance creation.
    pub async fn create_instance(&self, params: &CreateInstanceParams) -> Result<Operation> {
        let body = serde_json::to_value(params)?;
        let envelope = self
            .request(Method::Post, "/1.0/instances", &[], Some(&body), None)
            .await?;
        operation_from_envelope(envelope)
    }

    /// Full replacement update (PUT). `etag`, if provided, is sent as
    /// `If-Match` for optimistic concurrency; a stale ETag surfaces as
    /// `Error::PreconditionFailed`, not the generic `Error::Api`.
    pub async fn update_instance(
        &self,
        name: &str,
        new_definition: &serde_json::Value,
        etag: Option<&str>,
    ) -> Result<Operation> {
        let path = format!("/1.0/instances/{name}");
        let envelope = self
            .request(Method::Put, &path, &[], Some(new_definition), etag)
            .await
            .map_err(|err| precondition_failed_or(err, name))?;
        operation_from_envelope(envelope)
    }

    /// Partial update (PATCH) - use this instead of `update_instance` for
    /// small config changes, to avoid a GET-then-PUT round trip.
    pub async fn patch_instance(
        &self,
        name: &str,
        patch: &serde_json::Value,
        etag: Option<&str>,
    ) -> Result<Operation> {
        let path = format!("/1.0/instances/{name}");
        let envelope = self
            .request(Method::Patch, &path, &[], Some(patch), etag)
            .await
            .map_err(|err| precondition_failed_or(err, name))?;
        operation_from_envelope(envelope)
    }

    pub async fn delete_instance(&self, name: &str) -> Result<Operation> {
        let path = format!("/1.0/instances/{name}");
        let envelope = self.request(Method::Delete, &path, &[], None, None).await?;
        operation_from_envelope(envelope)
    }

    async fn set_state(&self, name: &str, action: &str) -> Result<Operation> {
        let path = format!("/1.0/instances/{name}/state");
        let body = serde_json::json!({ "action": action });
        let envelope = self
            .request(Method::Put, &path, &[], Some(&body), None)
            .await?;
        operation_from_envelope(envelope)
    }

    pub async fn start_instance(&self, name: &str) -> Result<Operation> {
        self.set_state(name, "start").await
    }

    pub async fn stop_instance(&self, name: &str) -> Result<Operation> {
        self.set_state(name, "stop").await
    }

    pub async fn restart_instance(&self, name: &str) -> Result<Operation> {
        self.set_state(name, "restart").await
    }

    pub async fn pause_instance(&self, name: &str) -> Result<Operation> {
        self.set_state(name, "freeze").await
    }

    pub async fn list_snapshots(&self, instance_name: &str, recursion: bool) -> Result<Vec<serde_json::Value>> {
        let recursion_value = recursion.to_string();
        let query = [("recursion", recursion_value.as_str())];
        let path = format!("/1.0/instances/{instance_name}/snapshots");
        let envelope = self.request(Method::Get, &path, &query, None, None).await?;
        match envelope {
            crate::transport::IncusEnvelope::Sync { metadata, .. } => Ok(serde_json::from_value(metadata)?),
            other => Err(Error::InvalidResponse(format!(
                "expected a sync list response, got {other:?}"
            ))),
        }
    }

    pub async fn create_snapshot(&self, instance_name: &str, snapshot_name: &str) -> Result<Operation> {
        let path = format!("/1.0/instances/{instance_name}/snapshots");
        let body = serde_json::json!({ "name": snapshot_name });
        let envelope = self
            .request(Method::Post, &path, &[], Some(&body), None)
            .await?;
        operation_from_envelope(envelope)
    }

    pub async fn delete_snapshot(&self, instance_name: &str, snapshot_name: &str) -> Result<Operation> {
        let path = format!("/1.0/instances/{instance_name}/snapshots/{snapshot_name}");
        let envelope = self.request(Method::Delete, &path, &[], None, None).await?;
        operation_from_envelope(envelope)
    }
}

#[cfg(test)]
#[path = "instances_tests.rs"]
mod tests;
```

- [ ] **Step 4: Make `unix::tests` reachable from `resources/instances_tests.rs`**

`instances_tests.rs` imports `crate::transport::unix::tests::{json_response, spawn_fake_daemon}` — confirm (from Task 2 Step 7) that `unix.rs`'s test module is `pub(crate) mod tests` with both helpers `pub(crate)`. If Task 2 was implemented exactly as written, this already works; no further change needed.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p incus-client resources::instances --all-features -- --nocapture`
Expected: all 10 tests pass.

Run: `cargo clippy -p incus-client --all-features -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/shared/incus-client
git commit -m "feat(incus-client): instances resource (CRUD, lifecycle, snapshots)"
```

---

## Task 6: Images / Networks / Storage / Projects Resources

**Files:**
- Modify: `crates/shared/incus-client/src/resources/images.rs` (implement — replaces Task 4's stub)
- Create: `crates/shared/incus-client/src/resources/images_tests.rs`
- Modify: `crates/shared/incus-client/src/resources/networks.rs` (implement)
- Create: `crates/shared/incus-client/src/resources/networks_tests.rs`
- Modify: `crates/shared/incus-client/src/resources/storage.rs` (implement)
- Create: `crates/shared/incus-client/src/resources/storage_tests.rs`
- Modify: `crates/shared/incus-client/src/resources/projects.rs` (implement)
- Create: `crates/shared/incus-client/src/resources/projects_tests.rs`

**Interfaces:**
- Consumes: `crate::transport::{Client, Method}` (Task 2), `crate::operations::{Operation, operation_from_envelope}` (Task 3).
- Produces: `pub struct Image { .. }`, `pub struct Network { .. }`, `pub struct StoragePool { .. }`, `pub struct StorageVolume { .. }`, `pub struct Project { .. }`, plus `list_*`/`get_*`/`create_*`/`update_*`/`delete_*` methods on `Client` for each. Every create/delete returns `Operation` (crate-wide convention — no per-resource sync exception). Every list method takes an explicit `recursion: bool`.

**Can run in parallel with Task 5** — disjoint files. These four resources share one uniform CRUD shape, so this task implements all four together rather than as separate tasks.

- [ ] **Step 1: Write the failing tests for images**

Create `crates/shared/incus-client/src/resources/images_tests.rs`:

```rust
use super::*;
use crate::config::ClientConfig;
use crate::transport::{Client, unix::tests::{json_response, spawn_fake_daemon}};

fn image_json(fingerprint: &str) -> String {
    format!(
        r#"{{"type":"sync","status":"Success","status_code":200,"metadata":{{"fingerprint":"{fingerprint}","public":false,"filename":"debian-12.tar.xz","size":123456,"architecture":"x86_64","created_at":"2026-01-01T00:00:00Z","uploaded_at":"2026-01-01T00:00:00Z","properties":{{}}}}}}"#
    )
}

fn operation_json(id: &str) -> String {
    format!(
        r#"{{"type":"async","status":"Operation created","status_code":100,"operation":"/1.0/operations/{id}","metadata":{{"id":"{id}","class":"task","status":"Running","status_code":103,"resources":{{}},"may_cancel":true,"err":null}}}}"#
    )
}

#[tokio::test]
async fn get_image_deserializes_the_documented_shape() {
    let body = image_json("abc123");
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| json_response("HTTP/1.1 200 OK", &body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let image = client.get_image("abc123").await.expect("get_image should succeed");
    assert_eq!(image.fingerprint, "abc123");
    assert_eq!(image.filename, "debian-12.tar.xz");
}

#[tokio::test]
async fn list_images_requires_explicit_recursion() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let body = r#"{"type":"sync","status":"Success","status_code":200,"metadata":[]}"#;
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        *seen.lock().unwrap() = String::from_utf8_lossy(&req).into_owned();
        json_response("HTTP/1.1 200 OK", body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    client.list_images(false).await.expect("list_images should succeed");
    assert!(seen_request.lock().unwrap().contains("recursion=false"));
}

#[tokio::test]
async fn create_image_returns_an_operation() {
    let id = uuid::Uuid::new_v4().to_string();
    let body = operation_json(&id);
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| json_response("HTTP/1.1 202 Accepted", &body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let op = client
        .create_image(&serde_json::json!({"source": {"type": "url", "url": "https://example.com/image.tar.xz"}}))
        .await
        .expect("create_image should return an Operation - image creation is documented as async");
    assert_eq!(op.id.to_string(), id);
}

#[tokio::test]
async fn delete_image_returns_an_operation() {
    let id = uuid::Uuid::new_v4().to_string();
    let body = operation_json(&id);
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| json_response("HTTP/1.1 202 Accepted", &body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let op = client.delete_image("abc123").await.expect("delete_image should return an Operation");
    assert_eq!(op.id.to_string(), id);
}

#[tokio::test]
async fn update_image_returns_an_operation() {
    let id = uuid::Uuid::new_v4().to_string();
    let body = operation_json(&id);
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| json_response("HTTP/1.1 202 Accepted", &body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let op = client
        .update_image("abc123", &serde_json::json!({"public": true}))
        .await
        .expect("update_image should return an Operation, per the crate-wide convention");
    assert_eq!(op.id.to_string(), id);
}
```

- [ ] **Step 2: Write the failing tests for networks**

Create `crates/shared/incus-client/src/resources/networks_tests.rs`:

```rust
use super::*;
use crate::config::ClientConfig;
use crate::transport::{Client, unix::tests::{json_response, spawn_fake_daemon}};

fn network_json(name: &str) -> String {
    format!(
        r#"{{"type":"sync","status":"Success","status_code":200,"metadata":{{"name":"{name}","type":"bridge","managed":true,"status":"Created","config":{{}}}}}}"#
    )
}

fn operation_json(id: &str) -> String {
    format!(
        r#"{{"type":"async","status":"Operation created","status_code":100,"operation":"/1.0/operations/{id}","metadata":{{"id":"{id}","class":"task","status":"Running","status_code":103,"resources":{{}},"may_cancel":true,"err":null}}}}"#
    )
}

#[tokio::test]
async fn get_network_deserializes_the_documented_shape() {
    let body = network_json("incusbr0");
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| json_response("HTTP/1.1 200 OK", &body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let network = client.get_network("incusbr0").await.expect("get_network should succeed");
    assert_eq!(network.name, "incusbr0");
    assert_eq!(network.network_type, "bridge");
}

#[tokio::test]
async fn list_networks_requires_explicit_recursion() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let body = r#"{"type":"sync","status":"Success","status_code":200,"metadata":[]}"#;
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        *seen.lock().unwrap() = String::from_utf8_lossy(&req).into_owned();
        json_response("HTTP/1.1 200 OK", body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    client.list_networks(true).await.expect("list_networks should succeed");
    assert!(seen_request.lock().unwrap().contains("recursion=true"));
}

#[tokio::test]
async fn create_and_delete_network_return_operations() {
    let id = uuid::Uuid::new_v4().to_string();
    let body = operation_json(&id);
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| json_response("HTTP/1.1 202 Accepted", &body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let create_op = client
        .create_network(&serde_json::json!({"name": "br1", "type": "bridge"}))
        .await
        .expect("create_network should return an Operation, per the crate-wide convention");
    assert_eq!(create_op.id.to_string(), id);

    let delete_op = client.delete_network("br1").await.expect("delete_network should return an Operation");
    assert_eq!(delete_op.id.to_string(), id);
}
```

- [ ] **Step 3: Write the failing tests for storage**

Create `crates/shared/incus-client/src/resources/storage_tests.rs`:

```rust
use super::*;
use crate::config::ClientConfig;
use crate::transport::{Client, unix::tests::{json_response, spawn_fake_daemon}};

fn pool_json(name: &str) -> String {
    format!(
        r#"{{"type":"sync","status":"Success","status_code":200,"metadata":{{"name":"{name}","driver":"zfs","status":"Created","config":{{}}}}}}"#
    )
}

fn volume_list_json() -> &'static str {
    r#"{"type":"sync","status":"Success","status_code":200,"metadata":[{"name":"vol1","type":"custom","content_type":"filesystem","config":{}}]}"#
}

fn operation_json(id: &str) -> String {
    format!(
        r#"{{"type":"async","status":"Operation created","status_code":100,"operation":"/1.0/operations/{id}","metadata":{{"id":"{id}","class":"task","status":"Running","status_code":103,"resources":{{}},"may_cancel":true,"err":null}}}}"#
    )
}

#[tokio::test]
async fn get_storage_pool_deserializes_the_documented_shape() {
    let body = pool_json("default");
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| json_response("HTTP/1.1 200 OK", &body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let pool = client.get_storage_pool("default").await.expect("get_storage_pool should succeed");
    assert_eq!(pool.name, "default");
    assert_eq!(pool.driver, "zfs");
}

#[tokio::test]
async fn list_storage_volumes_is_scoped_to_a_specific_pool() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        *seen.lock().unwrap() = String::from_utf8_lossy(&req).into_owned();
        json_response("HTTP/1.1 200 OK", volume_list_json())
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let volumes = client
        .list_storage_volumes("default", false)
        .await
        .expect("list_storage_volumes should succeed");

    assert_eq!(volumes.len(), 1);
    assert_eq!(volumes[0].name, "vol1");
    assert!(seen_request
        .lock()
        .unwrap()
        .contains("/1.0/storage-pools/default/volumes"));
}

#[tokio::test]
async fn create_and_delete_storage_pool_return_operations() {
    let id = uuid::Uuid::new_v4().to_string();
    let body = operation_json(&id);
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| json_response("HTTP/1.1 202 Accepted", &body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let create_op = client
        .create_storage_pool(&serde_json::json!({"name": "pool1", "driver": "zfs"}))
        .await
        .expect("create_storage_pool should return an Operation");
    assert_eq!(create_op.id.to_string(), id);

    let delete_op = client
        .delete_storage_pool("pool1")
        .await
        .expect("delete_storage_pool should return an Operation");
    assert_eq!(delete_op.id.to_string(), id);
}
```

- [ ] **Step 4: Write the failing tests for projects**

Create `crates/shared/incus-client/src/resources/projects_tests.rs`:

```rust
use super::*;
use crate::config::ClientConfig;
use crate::transport::{Client, unix::tests::{json_response, spawn_fake_daemon}};

fn project_json(name: &str) -> String {
    format!(
        r#"{{"type":"sync","status":"Success","status_code":200,"metadata":{{"name":"{name}","description":"","config":{{}}}}}}"#
    )
}

fn operation_json(id: &str) -> String {
    format!(
        r#"{{"type":"async","status":"Operation created","status_code":100,"operation":"/1.0/operations/{id}","metadata":{{"id":"{id}","class":"task","status":"Running","status_code":103,"resources":{{}},"may_cancel":true,"err":null}}}}"#
    )
}

#[tokio::test]
async fn get_project_deserializes_the_documented_shape() {
    let body = project_json("default");
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| json_response("HTTP/1.1 200 OK", &body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let project = client.get_project("default").await.expect("get_project should succeed");
    assert_eq!(project.name, "default");
}

#[tokio::test]
async fn list_projects_requires_explicit_recursion() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let body = r#"{"type":"sync","status":"Success","status_code":200,"metadata":[]}"#;
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        *seen.lock().unwrap() = String::from_utf8_lossy(&req).into_owned();
        json_response("HTTP/1.1 200 OK", body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    client.list_projects(false).await.expect("list_projects should succeed");
    assert!(seen_request.lock().unwrap().contains("recursion=false"));
}

#[tokio::test]
async fn create_and_delete_project_return_operations() {
    let id = uuid::Uuid::new_v4().to_string();
    let body = operation_json(&id);
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| json_response("HTTP/1.1 202 Accepted", &body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let create_op = client
        .create_project(&serde_json::json!({"name": "proj1"}))
        .await
        .expect("create_project should return an Operation, per the crate-wide convention");
    assert_eq!(create_op.id.to_string(), id);

    let delete_op = client.delete_project("proj1").await.expect("delete_project should return an Operation");
    assert_eq!(delete_op.id.to_string(), id);
}
```

- [ ] **Step 5: Run all four test files to verify they fail**

Run: `cargo test -p incus-client resources:: --all-features 2>&1 | head -80`
Expected: compile failure — none of `Image`/`Network`/`StoragePool`/`StorageVolume`/`Project` or their methods exist yet.

- [ ] **Step 6: Implement `resources/images.rs`**

Replace the stub with:

```rust
//! Image CRUD.

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::operations::{operation_from_envelope, Operation};
use crate::transport::{Client, Method};

#[derive(Debug, Clone, Deserialize)]
pub struct Image {
    pub fingerprint: String,
    pub public: bool,
    pub filename: String,
    pub size: i64,
    pub architecture: String,
    pub created_at: String,
    pub uploaded_at: String,
    #[serde(default)]
    pub properties: serde_json::Value,
}

impl Client {
    /// `recursion = true` fetches every image's full object in one call;
    /// `recursion = false` returns lightweight fingerprint/URL references.
    pub async fn list_images(&self, recursion: bool) -> Result<Vec<serde_json::Value>> {
        let recursion_value = recursion.to_string();
        let query = [("recursion", recursion_value.as_str())];
        let envelope = self.request(Method::Get, "/1.0/images", &query, None, None).await?;
        match envelope {
            crate::transport::IncusEnvelope::Sync { metadata, .. } => Ok(serde_json::from_value(metadata)?),
            other => Err(Error::InvalidResponse(format!("expected a sync list response, got {other:?}"))),
        }
    }

    pub async fn get_image(&self, fingerprint: &str) -> Result<Image> {
        let path = format!("/1.0/images/{fingerprint}");
        let envelope = self.request(Method::Get, &path, &[], None, None).await?;
        match envelope {
            crate::transport::IncusEnvelope::Sync { metadata, .. } => Ok(serde_json::from_value(metadata)?),
            other => Err(Error::InvalidResponse(format!("expected a sync image response, got {other:?}"))),
        }
    }

    /// Always async: image import/creation is documented as a long-running
    /// operation (fetching/unpacking a source, which can be a remote URL or
    /// a large upload).
    pub async fn create_image(&self, params: &serde_json::Value) -> Result<Operation> {
        let envelope = self.request(Method::Post, "/1.0/images", &[], Some(params), None).await?;
        operation_from_envelope(envelope)
    }

    pub async fn update_image(&self, fingerprint: &str, new_definition: &serde_json::Value) -> Result<Operation> {
        let path = format!("/1.0/images/{fingerprint}");
        let envelope = self.request(Method::Put, &path, &[], Some(new_definition), None).await?;
        operation_from_envelope(envelope)
    }

    pub async fn delete_image(&self, fingerprint: &str) -> Result<Operation> {
        let path = format!("/1.0/images/{fingerprint}");
        let envelope = self.request(Method::Delete, &path, &[], None, None).await?;
        operation_from_envelope(envelope)
    }
}

#[cfg(test)]
#[path = "images_tests.rs"]
mod tests;
```

- [ ] **Step 7: Implement `resources/networks.rs`**

Replace the stub with:

```rust
//! Network CRUD.

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::operations::{operation_from_envelope, Operation};
use crate::transport::{Client, Method};

#[derive(Debug, Clone, Deserialize)]
pub struct Network {
    pub name: String,
    #[serde(rename = "type")]
    pub network_type: String,
    pub managed: bool,
    pub status: String,
    #[serde(default)]
    pub config: serde_json::Value,
}

impl Client {
    pub async fn list_networks(&self, recursion: bool) -> Result<Vec<serde_json::Value>> {
        let recursion_value = recursion.to_string();
        let query = [("recursion", recursion_value.as_str())];
        let envelope = self.request(Method::Get, "/1.0/networks", &query, None, None).await?;
        match envelope {
            crate::transport::IncusEnvelope::Sync { metadata, .. } => Ok(serde_json::from_value(metadata)?),
            other => Err(Error::InvalidResponse(format!("expected a sync list response, got {other:?}"))),
        }
    }

    pub async fn get_network(&self, name: &str) -> Result<Network> {
        let path = format!("/1.0/networks/{name}");
        let envelope = self.request(Method::Get, &path, &[], None, None).await?;
        match envelope {
            crate::transport::IncusEnvelope::Sync { metadata, .. } => Ok(serde_json::from_value(metadata)?),
            other => Err(Error::InvalidResponse(format!("expected a sync network response, got {other:?}"))),
        }
    }

    /// Always async, per the crate-wide mutation-return convention (some
    /// network backends, e.g. OVN, provision asynchronously; treat every
    /// resource type uniformly rather than special-casing this one).
    pub async fn create_network(&self, params: &serde_json::Value) -> Result<Operation> {
        let envelope = self.request(Method::Post, "/1.0/networks", &[], Some(params), None).await?;
        operation_from_envelope(envelope)
    }

    pub async fn update_network(&self, name: &str, new_definition: &serde_json::Value) -> Result<Operation> {
        let path = format!("/1.0/networks/{name}");
        let envelope = self.request(Method::Put, &path, &[], Some(new_definition), None).await?;
        operation_from_envelope(envelope)
    }

    pub async fn delete_network(&self, name: &str) -> Result<Operation> {
        let path = format!("/1.0/networks/{name}");
        let envelope = self.request(Method::Delete, &path, &[], None, None).await?;
        operation_from_envelope(envelope)
    }
}

#[cfg(test)]
#[path = "networks_tests.rs"]
mod tests;
```

- [ ] **Step 8: Implement `resources/storage.rs`**

Replace the stub with:

```rust
//! Storage pool and volume CRUD. Volumes are scoped under a pool
//! (`/1.0/storage-pools/{pool}/volumes`) - Incus has no global cross-pool
//! volumes endpoint, so "list all volumes across all pools" is inherently a
//! list-pools-then-list-volumes-per-pool fan-out on the caller's part, not a
//! gap in this crate.

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::operations::{operation_from_envelope, Operation};
use crate::transport::{Client, Method};

#[derive(Debug, Clone, Deserialize)]
pub struct StoragePool {
    pub name: String,
    pub driver: String,
    pub status: String,
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageVolume {
    pub name: String,
    #[serde(rename = "type")]
    pub volume_type: String,
    pub content_type: String,
    #[serde(default)]
    pub config: serde_json::Value,
}

impl Client {
    pub async fn list_storage_pools(&self, recursion: bool) -> Result<Vec<serde_json::Value>> {
        let recursion_value = recursion.to_string();
        let query = [("recursion", recursion_value.as_str())];
        let envelope = self.request(Method::Get, "/1.0/storage-pools", &query, None, None).await?;
        match envelope {
            crate::transport::IncusEnvelope::Sync { metadata, .. } => Ok(serde_json::from_value(metadata)?),
            other => Err(Error::InvalidResponse(format!("expected a sync list response, got {other:?}"))),
        }
    }

    pub async fn get_storage_pool(&self, name: &str) -> Result<StoragePool> {
        let path = format!("/1.0/storage-pools/{name}");
        let envelope = self.request(Method::Get, &path, &[], None, None).await?;
        match envelope {
            crate::transport::IncusEnvelope::Sync { metadata, .. } => Ok(serde_json::from_value(metadata)?),
            other => Err(Error::InvalidResponse(format!("expected a sync storage pool response, got {other:?}"))),
        }
    }

    pub async fn create_storage_pool(&self, params: &serde_json::Value) -> Result<Operation> {
        let envelope = self.request(Method::Post, "/1.0/storage-pools", &[], Some(params), None).await?;
        operation_from_envelope(envelope)
    }

    pub async fn delete_storage_pool(&self, name: &str) -> Result<Operation> {
        let path = format!("/1.0/storage-pools/{name}");
        let envelope = self.request(Method::Delete, &path, &[], None, None).await?;
        operation_from_envelope(envelope)
    }

    /// Lists volumes within one pool. See the module doc comment for why
    /// there's no "list all volumes across all pools" convenience method.
    pub async fn list_storage_volumes(&self, pool_name: &str, recursion: bool) -> Result<Vec<StorageVolume>> {
        let recursion_value = recursion.to_string();
        let query = [("recursion", recursion_value.as_str())];
        let path = format!("/1.0/storage-pools/{pool_name}/volumes");
        let envelope = self.request(Method::Get, &path, &query, None, None).await?;
        match envelope {
            crate::transport::IncusEnvelope::Sync { metadata, .. } => Ok(serde_json::from_value(metadata)?),
            other => Err(Error::InvalidResponse(format!("expected a sync list response, got {other:?}"))),
        }
    }

    pub async fn create_storage_volume(&self, pool_name: &str, params: &serde_json::Value) -> Result<Operation> {
        let path = format!("/1.0/storage-pools/{pool_name}/volumes");
        let envelope = self.request(Method::Post, &path, &[], Some(params), None).await?;
        operation_from_envelope(envelope)
    }

    pub async fn delete_storage_volume(&self, pool_name: &str, volume_type: &str, volume_name: &str) -> Result<Operation> {
        let path = format!("/1.0/storage-pools/{pool_name}/volumes/{volume_type}/{volume_name}");
        let envelope = self.request(Method::Delete, &path, &[], None, None).await?;
        operation_from_envelope(envelope)
    }
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
```

- [ ] **Step 9: Implement `resources/projects.rs`**

Replace the stub with:

```rust
//! Project CRUD.

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::operations::{operation_from_envelope, Operation};
use crate::transport::{Client, Method};

#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub config: serde_json::Value,
}

impl Client {
    pub async fn list_projects(&self, recursion: bool) -> Result<Vec<serde_json::Value>> {
        let recursion_value = recursion.to_string();
        let query = [("recursion", recursion_value.as_str())];
        let envelope = self.request(Method::Get, "/1.0/projects", &query, None, None).await?;
        match envelope {
            crate::transport::IncusEnvelope::Sync { metadata, .. } => Ok(serde_json::from_value(metadata)?),
            other => Err(Error::InvalidResponse(format!("expected a sync list response, got {other:?}"))),
        }
    }

    pub async fn get_project(&self, name: &str) -> Result<Project> {
        let path = format!("/1.0/projects/{name}");
        let envelope = self.request(Method::Get, &path, &[], None, None).await?;
        match envelope {
            crate::transport::IncusEnvelope::Sync { metadata, .. } => Ok(serde_json::from_value(metadata)?),
            other => Err(Error::InvalidResponse(format!("expected a sync project response, got {other:?}"))),
        }
    }

    /// Always async, per the crate-wide mutation-return convention.
    pub async fn create_project(&self, params: &serde_json::Value) -> Result<Operation> {
        let envelope = self.request(Method::Post, "/1.0/projects", &[], Some(params), None).await?;
        operation_from_envelope(envelope)
    }

    pub async fn update_project(&self, name: &str, new_definition: &serde_json::Value) -> Result<Operation> {
        let path = format!("/1.0/projects/{name}");
        let envelope = self.request(Method::Put, &path, &[], Some(new_definition), None).await?;
        operation_from_envelope(envelope)
    }

    pub async fn delete_project(&self, name: &str) -> Result<Operation> {
        let path = format!("/1.0/projects/{name}");
        let envelope = self.request(Method::Delete, &path, &[], None, None).await?;
        operation_from_envelope(envelope)
    }
}

#[cfg(test)]
#[path = "projects_tests.rs"]
mod tests;
```

- [ ] **Step 10: Run every test in the crate**

Run: `cargo test -p incus-client --all-features -- --nocapture`
Expected: every test across all six tasks passes.

Run: `cargo test -p incus-client` (default features)
Expected: passes, `events` module not compiled.

Run: `cargo clippy -p incus-client --all-features -- -D warnings`
Expected: no warnings.

Run: `cargo fmt -p incus-client -- --check`
Expected: no diff.

Run: `cargo build --workspace`
Expected: the whole workspace still builds.

- [ ] **Step 11: Update the CHANGELOG with the full crate summary**

Edit `CHANGELOG.md`, replace the single bullet added in Task 1 with a fuller summary reflecting the complete crate (or add a follow-up bullet underneath it — either is fine, just make sure the `[Unreleased]` section accurately describes what shipped):

```markdown
- `incus-client` (crates/shared/incus-client) is now feature-complete for v1:
  Unix-socket transport, operation wait/cancel (including WebSocket events
  behind the `events` feature), and CRUD for instances (with lifecycle and
  snapshots), images, networks, storage pools/volumes, and projects. Remote
  mTLS transport and certificates CRUD are tracked separately for whenever a
  real remote consumer exists.
```

- [ ] **Step 12: Commit**

```bash
git add crates/shared/incus-client CHANGELOG.md
git commit -m "feat(incus-client): images, networks, storage, and projects resources"
```

---

## Self-Review

**Spec coverage** (against the beads epic `rmcp-template-hwu2` this plan implements, as revised after engineering review):
- Bead `.1` (scaffolding, error types, config) → Task 1. ✓
- Bead `.2` (unix-socket transport, revised: concurrency requirement, adversarial disconnect test, peer/path-integrity check, response-size cap) → Task 2. ✓
- Bead `.3` (operations + events, revised: wait/timeout re-poll semantics, backpressure-safe events, resources skeleton without certificates) → Tasks 3 and 4. ✓
- Bead `.4` (instances: CRUD, lifecycle, snapshots, revised: PATCH method, explicit recursion parameter, 412 handling) → Task 5. ✓
- Bead `.5` (images/networks/storage/projects, revised: crate-wide mutation convention, no certificates, storage volume nesting kept) → Task 6. ✓
- Epic-level crate-wide mutation-return convention → stated in Global Constraints and applied identically in Tasks 5 and 6. ✓
- Deferred: mTLS/TOFU/trust-token transport, certificates CRUD, `?filter=` support — correctly absent from every task; tracked in beads epic `rmcp-template-21b7`, not silently dropped.

**Placeholder scan:** no `TBD`/`TODO`/"implement later" markers exist in any task. One deliberate exception is flagged and resolved inline, not left open: Task 4 Step 3's first `subscribe_events` draft doesn't compile as originally composed (an unnecessary `MaybeTlsStream` re-wrap) — the note immediately below it gives the corrected, complete implementation rather than leaving the bug for the implementing engineer to discover. Every other code block is complete, working Rust as written.

**Type consistency:** `Client::request` (Task 2) → `IncusEnvelope` is consumed identically by `operation_from_envelope` (Task 3) and every `list_*`/`get_*` match arm in Tasks 5-6. `Operation`/`OperationClass` (Task 3) fields match every JSON test fixture across Tasks 3-6 exactly (`id`, `class`, `status`, `status_code`, `resources`, `metadata`, `may_cancel`, `err`). `WithEtag<T>` (Task 2) is used by `get_instance` (Task 5) with the same field names (`value`, `etag`). `Method::as_str()` values (`GET`/`POST`/`PUT`/`PATCH`/`DELETE`) match every resource method's HTTP verb choice. `Error::PreconditionFailed { resource }` (Task 1) is populated identically in `precondition_failed_or` (Task 5, the only place it's constructed).

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-07-17-incus-client-crate.md`.

You asked for `/work-it` after this plan — I'll invoke that next instead of the writing-plans skill's own default execution options (subagent-driven-development / executing-plans), since `/work-it` runs its own worktree + progress-tracked-PR + mandatory review-and-fix loop over the plan's tasks.
