mod dispatch;

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::process::Child;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::protocol::{ClientRequest, RequestId, ServerNotification, ServerRequest};
use tokio::io::AsyncWriteExt as _;

use crate::transport::{self, AsyncBufRead, AsyncWrite, OutgoingReply};
use crate::{Error, Result};

/// Default timeout for `CodexAppServerClient::call_request` (and therefore
/// every generated per-method wrapper). Override with
/// [`CodexAppServerClient::with_call_timeout`]. This bounds one request/response
/// round trip - it has nothing to do with how long a turn takes to finish
/// generating, since that streams via [`crate::Event::Notification`] instead
/// of blocking the request that started it.
pub const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(120);

/// Timeout for a single outgoing line write. A `write_all`/`flush` to a healthy
/// pipe or socket completes in microseconds; if it hasn't completed in this
/// long the peer is almost certainly stalled (backpressure with nobody
/// reading), and we'd rather tear the connection down than hang forever with
/// an ever-growing outgoing queue and no diagnostics. Tearing the connection
/// down always fails every *future* call immediately (`write_tx.send`
/// starts failing once the writer task exits). For [`CodexAppServerClient::spawn`]
/// it also fails every *pending* call promptly: killing the child closes its
/// stdout, the reader sees EOF, and that's what actually clears the pending
/// map. For [`CodexAppServerClient::connect_streams`]/[`CodexAppServerClient::connect_unix`]
/// there's no child to kill, so a write-stall alone doesn't touch the reader
/// task or the pending map - already-in-flight calls there still rely on
/// their own `call_timeout` instead.
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

/// Upper bound on how long the forwarding task spawned for one incoming
/// server->client request (see [`PendingServerRequest`]) will wait for a
/// reply before giving up on its own. This is a backstop for the case where a
/// caller holds a `PendingServerRequest` forever without ever dropping or
/// responding to it (e.g. stored in a collection and forgotten). Dropping one
/// (deliberately, via cancellation, or via a panic) always sends a fallback
/// error through its own `Drop` impl first, so this timeout only covers the
/// "never even dropped" case. Generous because these are often
/// human-in-the-loop approval/elicitation flows that can legitimately take a
/// while.
const PENDING_SERVER_REQUEST_TIMEOUT: Duration = Duration::from_secs(600);

/// Capacity of the internal channel from the reader task to [`EventStream`].
/// Bounded (rather than unbounded) so a stalled or absent consumer grows
/// memory by a fixed amount, not without bound, if events keep arriving
/// faster than [`EventStream::recv`] is called - see that type's doc comment
/// for the drop policy once this fills up.
const EVENTS_CHANNEL_CAPACITY: usize = 1024;

/// Capacity of the internal channel from callers (and the reader task's own
/// reply-forwarding) to the writer task. Bounded for the same reason as
/// [`EVENTS_CHANNEL_CAPACITY`]: without a cap, a caller issuing many
/// concurrent requests while the peer write is slow (anywhere up to
/// [`WRITE_TIMEOUT`] before the connection gets torn down) could grow this
/// queue by an unbounded number of serialized-but-unwritten lines. A full
/// queue surfaces as [`Error::TransportClosed`] to the caller that tried to
/// enqueue onto it - not perfectly accurate wording for "backed up" versus
/// "closed," but by the time this queue is actually full the connection is
/// in comparably serious trouble, and treating it as unusable is reasonable
/// without adding a dedicated error variant for a narrow, rare case.
const WRITE_CHANNEL_CAPACITY: usize = 1024;

type PendingSender = oneshot::Sender<std::result::Result<serde_json::Value, Error>>;
type PendingMap = Arc<Mutex<HashMap<i64, PendingSender>>>;

/// Locks `pending`, recovering from mutex poisoning rather than panicking a
/// second time. A panic while this lock is held elsewhere in the crate can't
/// leave the underlying `HashMap` in a logically-broken state - every
/// operation on it (`insert`/`remove`/`clear`) either fully completes or
/// doesn't start - so recovering the guard and continuing is safe, and it
/// avoids turning one unrelated bug into a crate-wide cascade of
/// poisoned-lock panics on every subsequent call.
fn lock_pending(pending: &PendingMap) -> MutexGuard<'_, HashMap<i64, PendingSender>> {
    pending
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// An event pushed from the app-server connection. Deliberately *not*
/// `#[non_exhaustive]` (contrast [`Error`], which explains its own opposite
/// choice) - this models a closed, stable three-way split of "shape of thing
/// the app-server can send," not an open-ended taxonomy, so callers matching
/// on it exhaustively (as this crate's own code does everywhere) get a
/// compile error if a variant is ever added, rather than silently ignoring
/// the new case via a wildcard arm.
#[derive(Debug)]
pub enum Event {
    /// A fire-and-forget server notification (`turn/completed`, `item/started`, etc.).
    Notification(ServerNotification),
    /// A request the app-server expects a reply to (approvals, elicitation, etc.).
    Request(PendingServerRequest),
    /// The transport closed (EOF or the child process exited). No further
    /// events will be produced; the [`EventStream`] is exhausted.
    Closed,
}

/// Receives [`Event`]s from one app-server connection.
///
/// Own exactly one `EventStream` per connection and keep draining it (even if
/// you only care about requests, not notifications). The channel between the
/// reader task and this stream is bounded (see `EVENTS_CHANNEL_CAPACITY`):
/// a slow consumer just grows a fixed-size backlog, but a consumer that stops
/// draining entirely will eventually cause the reader task to drop events
/// once that backlog fills. Drop policy when full:
/// - [`Event::Notification`]: dropped and logged - fire-and-forget by design.
/// - [`Event::Request`]: **not** silently dropped - this crate sends a
///   fallback error reply on the app-server's behalf first (so it isn't left
///   hanging), then drops the event.
/// - [`Event::Closed`]: dropped and logged, but harmless - once the reader
///   task ends it drops its sender, so [`Self::recv`] still observes the
///   connection closing (as `None`) even without the explicit event.
pub struct EventStream {
    rx: mpsc::Receiver<Event>,
}

impl EventStream {
    pub async fn recv(&mut self) -> Option<Event> {
        self.rx.recv().await
    }
}

/// A server->client request the app-server sent us (approvals, elicitation,
/// etc.), paired with a one-shot reply channel. Obtain these from
/// [`EventStream::recv`] as an [`Event::Request`].
///
/// Every `PendingServerRequest` gets exactly one reply, no matter what
/// happens to it: call [`Self::respond`] or [`Self::respond_error`] to send
/// your own, or just let it drop - deliberately, via cancellation, or via a
/// panic unwinding through it - and its [`Drop`] impl sends a generic
/// fallback error on your behalf. The app-server is never left permanently
/// unanswered either way. Prefer responding explicitly and promptly when you
/// can; the fallback exists so a bug, an unhandled case, or a task
/// abandoning this value doesn't turn into a silent hang.
pub struct PendingServerRequest {
    pub request: ServerRequest,
    reply_tx: Option<oneshot::Sender<OutgoingReply>>,
}

impl std::fmt::Debug for PendingServerRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingServerRequest")
            .field("request", &self.request)
            .finish_non_exhaustive()
    }
}

impl PendingServerRequest {
    #[cfg(all(test, feature = "rest"))]
    pub(crate) fn for_test(request: ServerRequest) -> Self {
        let (reply_tx, _reply_rx) = oneshot::channel::<OutgoingReply>();
        Self {
            request,
            reply_tx: Some(reply_tx),
        }
    }

    /// The `RequestId` the app-server expects echoed back in the reply.
    pub fn id(&self) -> &RequestId {
        self.request.id()
    }

    /// The wire method name, e.g. `"execCommandApproval"`.
    pub fn method_name(&self) -> &'static str {
        self.request.method_name()
    }

    /// The name of the `crate::protocol` type [`Self::respond`]'s `result`
    /// must serialize to for this specific request, e.g.
    /// `"ExecCommandApprovalResponse"`. Prefer this over guessing from
    /// [`Self::method_name`] - the naming convention (`PascalCase(method) +
    /// "Response"`) doesn't hold for every method (e.g.
    /// `"item/tool/call"` expects `DynamicToolCallResponse`, not
    /// `ItemToolCallResponse`).
    pub fn expected_response_type_name(&self) -> &'static str {
        self.request.expected_response_type_name()
    }

    /// Send a successful reply. `result` must serialize to the response
    /// shape the app-server expects for this specific method - see
    /// [`Self::expected_response_type_name`] for exactly which
    /// `crate::protocol` type that is. `Err` only means `result` failed to
    /// serialize; a returned `Ok(())` means the reply was accepted, not that
    /// it was necessarily delivered - if the forwarding task already gave up
    /// on this request (its own timeout already fired), delivery is no
    /// longer possible and that's logged, but it's too late to matter which
    /// of the two happened.
    pub fn respond(mut self, result: impl serde::Serialize) -> Result<()> {
        let id = self.id().clone();
        let value = serde_json::to_value(result)?;
        self.send_reply(OutgoingReply::Result { id, result: value });
        Ok(())
    }

    /// Send an error reply. Unlike [`Self::respond`], this can't fail - it
    /// builds a fixed-shape error object rather than serializing an
    /// arbitrary caller-supplied value - so it deliberately returns `()`
    /// rather than a `Result` that could only ever be `Ok`.
    pub fn respond_error(
        mut self,
        code: i64,
        message: impl Into<String>,
        data: Option<serde_json::Value>,
    ) {
        let id = self.id().clone();
        self.send_reply(OutgoingReply::Error {
            id,
            code,
            message: message.into(),
            data,
        });
    }

    /// Takes `reply_tx` (leaving `None` so [`Drop`] becomes a no-op) and
    /// sends `reply` through it, logging - rather than silently discarding -
    /// the case where the forwarding task already gave up and dropped its
    /// receiving end.
    fn send_reply(&mut self, reply: OutgoingReply) {
        let Some(tx) = self.reply_tx.take() else {
            return;
        };
        if tx.send(reply).is_err() {
            tracing::debug!(
                method = self.method_name(),
                "reply channel already closed (the forwarding task must have already given up, \
                 e.g. its own timeout fired) - this reply was accepted but could not be delivered"
            );
        }
    }
}

impl Drop for PendingServerRequest {
    fn drop(&mut self) {
        // `respond`/`respond_error` already took `reply_tx` (leaving `None`)
        // if either was called - only a bare drop (deliberate, cancelled, or
        // unwinding through a panic) reaches this with `Some` still set.
        let Some(tx) = self.reply_tx.take() else {
            return;
        };
        let id = self.request.id().clone();
        let _ = tx.send(OutgoingReply::Error {
            id,
            code: -32000,
            message: "codex-app-server-client: PendingServerRequest was dropped without a response"
                .to_string(),
            data: None,
        });
    }
}

/// Drop signal decoupled from any internal task's channel clones. Only
/// [`CodexAppServerClient`] holds (a clone of) the `Arc` wrapping this; the
/// reader task and per-request reply-forwarding tasks intentionally do *not*
/// hold *this* handle (though the reader task holds its own [`CancellationToken`]
/// clone directly - see `connect` - so it can also trigger shutdown itself on
/// EOF/error, not just observe it), so the connection's lifetime tracks "the
/// caller still holds a client handle, or the connection is known to be dead"
/// rather than "some internal task still happens to hold a sender."
struct ShutdownOnDrop(CancellationToken);

impl Drop for ShutdownOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// Async client for the Codex app-server v2 JSON-RPC protocol.
///
/// Cheap to `Clone`; every clone shares the same underlying connection and
/// pending-request table. Construct one with [`Self::spawn`] (launches
/// `codex app-server` as a child process, the common case), [`Self::connect_streams`]
/// (bring your own duplex byte stream), or [`Self::connect_unix`] (a Unix
/// socket to an already-running `codex app-server daemon`).
///
/// You must complete the `initialize` / `initialized` handshake before calling
/// any other method - the app-server rejects everything else on a fresh
/// connection with a `"Not initialized"` error. See [`Self::initialize`] and
/// [`Self::send_initialized`].
#[derive(Clone)]
pub struct CodexAppServerClient {
    write_tx: mpsc::Sender<String>,
    pending: PendingMap,
    next_id: Arc<AtomicI64>,
    call_timeout: Duration,
    _lifetime: Arc<ShutdownOnDrop>,
}

impl CodexAppServerClient {
    /// Spawns `command app-server` (default `command = "codex"`) with stdio
    /// piped and connects to it over the stdio JSONL transport.
    ///
    /// `extra_args` are appended after `app-server`, e.g.
    /// `["--enable".into(), "some_feature".into()]` or
    /// `["-c".into(), r#"model="gpt-5.4""#.into()]`.
    ///
    /// The child process is killed (`kill_on_drop`) once every clone of the
    /// returned client has been dropped - dropping the last clone always
    /// terminates the child, whether or not it's currently mid-write. It's
    /// also reaped promptly and automatically the moment the connection is
    /// otherwise known to be dead: a transport read error, clean EOF (e.g.
    /// the child crashed or exited), or a write that exceeds the internal
    /// stall timeout - none of these require the caller to drop the client or
    /// issue another call first.
    pub fn spawn(command: &str, extra_args: &[String]) -> Result<(Self, EventStream)> {
        let (stdin, reader, child) = transport::spawn_app_server(command, extra_args)?;
        Ok(Self::connect(reader, stdin, Some(child)))
    }

    /// Connects to an already-open duplex stream (e.g. any
    /// `AsyncBufRead + AsyncWrite` pair) speaking the same NDJSON JSON-RPC
    /// protocol. See also [`Self::connect_unix`] for the common case of a
    /// `codex app-server daemon`'s Unix domain socket.
    pub fn connect_streams<R, W>(reader: R, writer: W) -> (Self, EventStream)
    where
        R: AsyncBufRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        Self::connect(reader, writer, None)
    }

    /// Connects to a `codex app-server` (or `codex app-server daemon`)
    /// listening on a Unix domain socket (`--listen unix://PATH`).
    #[cfg(unix)]
    pub async fn connect_unix(path: impl AsRef<std::path::Path>) -> Result<(Self, EventStream)> {
        let stream = tokio::net::UnixStream::connect(path).await?;
        let (writer, reader) = transport::split_unix_stream(stream);
        Ok(Self::connect(reader, writer, None))
    }

    /// Returns a client that applies `timeout` to every request/response call
    /// instead of [`DEFAULT_CALL_TIMEOUT`]. Cloning preserves the timeout;
    /// mixing timeouts across clones of the same connection is fine (it's a
    /// per-call setting checked in `Self::call_request`, not a property of
    /// the shared connection state).
    pub fn with_call_timeout(mut self, timeout: Duration) -> Self {
        self.call_timeout = timeout;
        self
    }

    fn connect<R, W>(mut reader: R, mut writer: W, child: Option<Child>) -> (Self, EventStream)
    where
        R: AsyncBufRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (write_tx, mut write_rx) = mpsc::channel::<String>(WRITE_CHANNEL_CAPACITY);
        let (events_tx, events_rx) = mpsc::channel::<Event>(EVENTS_CHANNEL_CAPACITY);
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let cancel = CancellationToken::new();

        // Writer task: owns the write half + (if spawned) the child process,
        // so the process stays alive exactly as long as this task is running.
        // Exits (and so kills the child) on: cancellation - triggered either by
        // the last CodexAppServerClient clone being dropped (`ShutdownOnDrop`)
        // *or* by the reader task detecting the connection is dead (EOF/error,
        // see below) - a write that exceeds WRITE_TIMEOUT (peer stalled), a
        // write I/O error, or every sender having been dropped (fallback -
        // should be unreachable in practice since the reader task and
        // forwarding tasks are careful not to hold long-lived clones past
        // their own task's natural lifetime, but a real fallback rather than
        // an assumption).
        let writer_cancel = cancel.clone();
        tokio::spawn(async move {
            let _child = child; // kept alive here; dropped (kill_on_drop) when this task ends
            loop {
                tokio::select! {
                    biased;
                    _ = writer_cancel.cancelled() => break,
                    maybe_line = write_rx.recv() => {
                        let Some(line) = maybe_line else { break };
                        match tokio::time::timeout(WRITE_TIMEOUT, transport::write_line(&mut writer, &line)).await {
                            Ok(Ok(())) => {}
                            Ok(Err(err)) => {
                                tracing::warn!(error = %err, "app-server transport write error; closing connection");
                                break;
                            }
                            Err(_elapsed) => {
                                tracing::warn!(timeout = ?WRITE_TIMEOUT, "app-server write stalled past the timeout; closing connection");
                                break;
                            }
                        }
                    }
                }
            }
            // Explicitly shut down the write half rather than relying on drop
            // alone: for a spawned child, this closes its stdin, giving it a
            // chance to notice and exit cleanly before `kill_on_drop` resorts
            // to a hard kill; for split-stream transports (`connect_streams`,
            // `connect_unix`), a bare drop doesn't reliably signal EOF to the
            // peer when a `ReadHalf` derived from the same underlying stream
            // is still alive elsewhere (as it is here, in the reader task).
            let _ = writer.shutdown().await;
        });

        // Reader task: the only place incoming lines are parsed and dispatched.
        // It needs its own `write_tx` clone to send replies (error responses
        // to undecodable requests, forwarded PendingServerRequest replies),
        // so it necessarily holds one for its whole lifetime - that's fine
        // now, because the writer task's shutdown no longer depends on every
        // `write_tx` clone being dropped (see `ShutdownOnDrop`'s doc comment
        // and the `select!` above); it depends on `cancel`.
        let pending_reader = pending.clone();
        let write_tx_for_reader = write_tx.clone();
        let reader_cancel = cancel.clone();
        tokio::spawn(async move {
            // Guarantees the three cleanup steps below run on *every* exit
            // from the loop - clean EOF, a transport error, cancellation, or
            // a panic unwinding out of `dispatch_incoming_line` - rather than
            // only the two normal `break` paths the loop was originally
            // written to reach. Without this, a panic here would silently
            // orphan the connection: pending calls never resolve (each rides
            // out its own timeout instead), and the writer task (and, for a
            // spawned child, the process) is never reaped, because nothing
            // else calls `reader_cancel.cancel()` on this task's behalf. Owns
            // cheap clones (`Arc`/`Sender`/`CancellationToken`) rather than
            // borrowing so it has no lifetime relationship to the rest of
            // this function's locals to reason about.
            struct ReaderCleanup {
                pending: PendingMap,
                events_tx: mpsc::Sender<Event>,
                reader_cancel: CancellationToken,
            }
            impl Drop for ReaderCleanup {
                fn drop(&mut self) {
                    lock_pending(&self.pending).clear(); // drops senders -> pending calls see TransportClosed
                    if let Err(err) = self.events_tx.try_send(Event::Closed) {
                        // Harmless: dropping `events_tx` (this task ending)
                        // still makes `EventStream::recv` observe closure as
                        // `None`.
                        tracing::debug!(
                            ?err,
                            "event channel full/closed while sending Event::Closed"
                        );
                    }
                    // Proactively tear down the writer task (and reap a
                    // spawned child) the moment we know the connection is
                    // dead, rather than waiting for the caller to notice
                    // `Event::Closed` and drop the client, or for the
                    // writer's own next write attempt to fail.
                    self.reader_cancel.cancel();
                }
            }
            let _cleanup = ReaderCleanup {
                pending: pending_reader.clone(),
                events_tx: events_tx.clone(),
                reader_cancel: reader_cancel.clone(),
            };

            let mut buf = String::new();
            loop {
                // Raced against `reader_cancel` (not just relied on via
                // EOF/error) so a caller-initiated shutdown - the last client
                // clone dropping - terminates this task promptly even when
                // the peer never notices the writer's half-close and so never
                // sends EOF back (a real risk for `connect_streams`/
                // `connect_unix`, which - unlike a spawned child - have no
                // `kill_on_drop` to force the issue).
                tokio::select! {
                    biased;
                    _ = reader_cancel.cancelled() => break,
                    result = transport::read_line(&mut reader, &mut buf) => {
                        match result {
                            Ok(0) => break, // EOF
                            Ok(_) => {
                                let line = buf.trim();
                                if line.is_empty() {
                                    continue;
                                }
                                dispatch::dispatch_incoming_line(
                                    line,
                                    &pending_reader,
                                    &events_tx,
                                    &write_tx_for_reader,
                                    &reader_cancel,
                                );
                            }
                            Err(err) => {
                                tracing::warn!(error = %err, "app-server transport read error");
                                break;
                            }
                        }
                    }
                }
            }
        });

        let client = CodexAppServerClient {
            write_tx,
            pending,
            next_id: Arc::new(AtomicI64::new(0)),
            call_timeout: DEFAULT_CALL_TIMEOUT,
            _lifetime: Arc::new(ShutdownOnDrop(cancel)),
        };
        (client, EventStream { rx: events_rx })
    }

    /// Issues one typed request and awaits its response as a raw JSON value,
    /// bounded by [`Self::call_timeout`] (see [`Self::with_call_timeout`]).
    /// Used by the generated per-method wrapper functions
    /// (`thread_start`, `turn_start`, ...); most callers should use those
    /// instead of this directly.
    pub(crate) async fn call_request(
        &self,
        build: impl FnOnce(RequestId) -> ClientRequest,
    ) -> Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = build(RequestId::Int64(id));
        let line = serde_json::to_string(&request)?;
        self.call_serialized_request(id, line).await
    }

    /// Issues one raw JSON-RPC method call and returns its raw `result` value.
    ///
    /// This is the escape hatch for bridges and generated surfaces that need
    /// to call app-server methods dynamically. Prefer the typed generated
    /// wrappers when the method is known at compile time.
    pub async fn call_raw_method(
        &self,
        method: impl Into<String>,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut request = serde_json::Map::new();
        request.insert("id".to_owned(), serde_json::Value::from(id));
        request.insert(
            "method".to_owned(),
            serde_json::Value::String(method.into()),
        );
        if !params.is_null() {
            request.insert("params".to_owned(), params);
        }
        let line = serde_json::to_string(&request)?;
        self.call_serialized_request(id, line).await
    }

    async fn call_serialized_request(&self, id: i64, line: String) -> Result<serde_json::Value> {
        let (tx, rx) = oneshot::channel();
        lock_pending(&self.pending).insert(id, tx);

        // Removes this id's map entry when this guard drops, which happens
        // whether `call_request` returns normally, is cancelled (e.g. wrapped
        // in `tokio::time::timeout` or a `select!` branch that loses), or
        // panics. Without this, a cancelled call whose response never arrives
        // (or arrives after the caller already gave up) leaks its map entry
        // and `oneshot::Sender` for the life of the connection. Redundant
        // (and harmless - `HashMap::remove` on a missing key is a no-op) on
        // the normal-completion path, where `dispatch_incoming_line` already
        // removed the entry.
        struct RemoveOnDrop<'a> {
            pending: &'a PendingMap,
            id: i64,
        }
        impl Drop for RemoveOnDrop<'_> {
            fn drop(&mut self) {
                lock_pending(self.pending).remove(&self.id);
            }
        }
        let _guard = RemoveOnDrop {
            pending: &self.pending,
            id,
        };

        if self.write_tx.try_send(line).is_err() {
            return Err(Error::TransportClosed);
        }

        match tokio::time::timeout(self.call_timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_recv_error)) => Err(Error::TransportClosed),
            Err(_elapsed) => Err(Error::Timeout {
                after: self.call_timeout,
            }),
        }
    }

    /// Sends the `initialized` notification. Call this exactly once,
    /// immediately after [`Self::initialize`] succeeds - the app-server
    /// rejects every other method until it's received.
    pub fn send_initialized(&self) -> Result<()> {
        let line = serde_json::to_string(&serde_json::json!({ "method": "initialized" }))?;
        self.write_tx
            .try_send(line)
            .map_err(|_| Error::TransportClosed)
    }
}

include!(concat!(env!("OUT_DIR"), "/methods_generated.rs"));

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    /// Regression test: `expected_response_type_name()` must resolve to the
    /// *actual* response type even for methods where the naive
    /// `PascalCase(method) + "Response"` convention is wrong -
    /// `item/tool/call` is one of the 6 (of 11) server-request methods that
    /// need the `RESPONSE_OVERRIDES` table in
    /// `xtask/src/codex_schema/naming.rs` (it expects `DynamicToolCallResponse`,
    /// not the naive `ItemToolCallResponse`).
    #[tokio::test]
    async fn expected_response_type_name_resolves_irregular_names_correctly() {
        let (reply_tx, _reply_rx) = oneshot::channel::<OutgoingReply>();
        let pending = PendingServerRequest {
            request: ServerRequest::ItemToolCall {
                id: RequestId::Int64(1),
                params: crate::protocol::DynamicToolCallParams {
                    arguments: serde_json::json!({}),
                    call_id: "call-1".into(),
                    namespace: None,
                    thread_id: "thr_test".into(),
                    tool: "some_tool".into(),
                    turn_id: "turn_1".into(),
                },
            },
            reply_tx: Some(reply_tx),
        };
        assert_eq!(
            pending.expected_response_type_name(),
            "DynamicToolCallResponse"
        );
    }

    /// `PendingServerRequest::respond()` must serialize `result` and put it
    /// on the wire as `{"id": ..., "result": ...}` - the exact shape the
    /// app-server expects for a reply to one of its requests. Constructs the
    /// pair directly (bypassing the full dispatch/connection machinery,
    /// which is exercised by other tests) to check the wire format in
    /// isolation.
    #[tokio::test]
    async fn respond_puts_the_correct_shape_on_the_wire() {
        let (reply_tx, reply_rx) = oneshot::channel::<OutgoingReply>();
        let pending = PendingServerRequest {
            request: ServerRequest::CurrentTimeRead {
                id: RequestId::Int64(7),
                params: crate::protocol::CurrentTimeReadParams {
                    thread_id: "thr_test".into(),
                },
            },
            reply_tx: Some(reply_tx),
        };
        assert_eq!(pending.method_name(), "currentTime/read");
        assert_eq!(pending.id(), &RequestId::Int64(7));

        pending
            .respond(serde_json::json!({ "currentTimeAt": 12345 }))
            .expect("respond should succeed for a plain serializable value");

        let reply = reply_rx
            .await
            .expect("forwarding channel should receive the reply");
        let line = reply
            .into_line()
            .expect("reply should serialize to a wire line");
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            parsed,
            serde_json::json!({ "id": 7, "result": { "currentTimeAt": 12345 } })
        );
    }

    /// Same as above but for the error-reply path.
    #[tokio::test]
    async fn respond_error_puts_the_correct_shape_on_the_wire() {
        let (reply_tx, reply_rx) = oneshot::channel::<OutgoingReply>();
        let pending = PendingServerRequest {
            request: ServerRequest::CurrentTimeRead {
                id: RequestId::String("req-1".into()),
                params: crate::protocol::CurrentTimeReadParams {
                    thread_id: "thr_test".into(),
                },
            },
            reply_tx: Some(reply_tx),
        };

        pending.respond_error(-32000, "denied", None);

        let reply = reply_rx
            .await
            .expect("forwarding channel should receive the reply");
        let line = reply
            .into_line()
            .expect("reply should serialize to a wire line");
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            parsed,
            serde_json::json!({ "id": "req-1", "error": { "code": -32000, "message": "denied", "data": null } })
        );
    }

    /// Regression test: `PendingServerRequest`'s `Drop` impl must send a
    /// fallback error reply when a value is dropped without ever calling
    /// `.respond()`/`.respond_error()` - the whole point of moving from a
    /// bare `oneshot::Sender` field to `Option<...>` + `Drop`. Constructs the
    /// pair directly (bypassing the full dispatch/connection machinery) to
    /// check the `Drop`-triggered wire shape in isolation.
    #[tokio::test]
    async fn dropping_a_pending_server_request_without_responding_sends_a_fallback_error() {
        let (reply_tx, reply_rx) = oneshot::channel::<OutgoingReply>();
        let pending = PendingServerRequest {
            request: ServerRequest::CurrentTimeRead {
                id: RequestId::Int64(42),
                params: crate::protocol::CurrentTimeReadParams {
                    thread_id: "thr_test".into(),
                },
            },
            reply_tx: Some(reply_tx),
        };

        drop(pending); // no .respond()/.respond_error() call

        let reply = reply_rx
            .await
            .expect("Drop must send a fallback reply, not just drop the sender silently");
        let line = reply
            .into_line()
            .expect("fallback reply should serialize to a wire line");
        let parsed: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["id"], 42);
        assert_eq!(parsed["error"]["code"], -32000);
        assert!(
            parsed["error"]["message"]
                .as_str()
                .unwrap()
                .contains("dropped without a response"),
            "unexpected fallback message: {parsed}"
        );
    }

    /// `connect_unix` must complete a real handshake over an actual Unix
    /// domain socket - the only transport constructor not otherwise exercised
    /// by the `connect_streams`-based tests in this module (or by the live
    /// `tests/smoke.rs`, which only spawns a child over stdio).
    #[cfg(unix)]
    #[tokio::test]
    async fn connect_unix_completes_a_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("codex-app-server-client-test.sock");
        let listener = tokio::net::UnixListener::bind(&socket_path).unwrap();

        let accept_task = tokio::spawn(async move {
            let (stream, _addr) = listener.accept().await.unwrap();
            let (read_half, mut write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            let request: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["method"], "memory/reset");
            let id = request["id"].clone();
            // MemoryResetResponse wraps a JSON object (`serde_json::Map`), not `null`.
            let response = serde_json::json!({ "id": id, "result": {} });
            write_half
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        });

        let (client, _events) = CodexAppServerClient::connect_unix(&socket_path)
            .await
            .expect("connect_unix should dial the listening socket");
        client
            .memory_reset()
            .await
            .expect("round trip over the Unix socket should succeed");

        accept_task.await.unwrap();
    }

    /// Regression test: dropping a `oneshot::Sender` wakes its paired
    /// `Receiver` immediately with an error - it does not require the
    /// `Receiver`'s side of a `tokio::time::timeout` to elapse. This is the
    /// exact mechanism `PendingServerRequest`'s forwarding task relies on to
    /// resolve promptly (not after `PENDING_SERVER_REQUEST_TIMEOUT`) when a
    /// caller drops a `PendingServerRequest` without responding.
    #[tokio::test]
    async fn dropping_a_oneshot_sender_resolves_the_receiver_immediately_not_via_timeout() {
        let (tx, rx) = oneshot::channel::<()>();
        drop(tx);
        let result = tokio::time::timeout(Duration::from_secs(600), rx).await;
        assert!(
            matches!(result, Ok(Err(_))),
            "dropping the sender should resolve the receiver immediately with an error, \
             not by waiting out the 600s timeout - got {result:?}"
        );
    }

    /// Regression test: dropping a `PendingServerRequest` without responding
    /// must not wedge the connection - the reader must keep dispatching
    /// subsequent server->client requests normally afterward. Uses a real
    /// `currentTime/read` wire line (the simplest `ServerRequest` variant -
    /// `CurrentTimeReadParams` has exactly one required field) written
    /// directly into the duplex to exercise the actual `dispatch_incoming_line`
    /// path, not a hand-rolled shortcut.
    #[tokio::test]
    async fn an_abandoned_pending_server_request_does_not_wedge_the_connection() {
        let (client_io, mut server_io) = tokio::io::duplex(4096);
        let (client_read, client_write) = tokio::io::split(client_io);
        let (_client, mut events) =
            CodexAppServerClient::connect_streams(BufReader::new(client_read), client_write);

        let send_current_time_read = |id: i64| {
            format!(
                r#"{{"method":"currentTime/read","id":{id},"params":{{"threadId":"thr_test"}}}}"#
            )
        };

        server_io
            .write_all(format!("{}\n", send_current_time_read(1)).as_bytes())
            .await
            .unwrap();
        let first = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("first event within timeout")
            .expect("event stream still open");
        let Event::Request(pending) = first else {
            panic!("expected Event::Request, got something else");
        };
        drop(pending); // abandoned without .respond()/.respond_error()

        server_io
            .write_all(format!("{}\n", send_current_time_read(2)).as_bytes())
            .await
            .unwrap();
        let second = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("second event within timeout - connection must not be wedged by the abandoned request")
            .expect("event stream still open");
        assert!(
            matches!(second, Event::Request(_)),
            "expected a second Event::Request after abandoning the first, got something else"
        );
    }

    /// Regression test: when `EventStream` is dropped, an incoming
    /// server->client request must still get a JSON-RPC error reply on the
    /// wire - never silently dropped - exercising the `events_tx.try_send`
    /// failure branch in `dispatch::dispatch_incoming_line` (the behavioral
    /// change this session's channel-bounding fix was about; with the
    /// previous unbounded channel this branch could never be reached at
    /// all).
    #[tokio::test]
    async fn a_server_request_gets_a_fallback_error_reply_when_the_event_stream_is_dropped() {
        let (client_io, mut server_io) = tokio::io::duplex(4096);
        let (client_read, client_write) = tokio::io::split(client_io);
        let (_client, events) =
            CodexAppServerClient::connect_streams(BufReader::new(client_read), client_write);
        drop(events); // closes the events_tx receiver -> try_send returns Closed

        server_io
            .write_all(br#"{"method":"currentTime/read","id":9,"params":{"threadId":"thr_test"}}"#)
            .await
            .unwrap();
        server_io.write_all(b"\n").await.unwrap();

        let mut buf = [0u8; 4096];
        let n = tokio::time::timeout(Duration::from_secs(5), server_io.read(&mut buf))
            .await
            .expect("should get a reply promptly, not a hang")
            .unwrap();
        let reply: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();
        assert_eq!(reply["id"], 9);
        assert_eq!(reply["error"]["code"], -32000);
    }

    /// Regression test: the reader task detecting a dead connection (EOF from
    /// the peer, here simulated by dropping the peer side entirely) must
    /// proactively reap the writer task - not just when the caller eventually
    /// drops the client. Observed via `send_initialized` failing with
    /// `TransportClosed` once the writer task's channel receiver has dropped,
    /// which only happens once that task has actually exited.
    #[tokio::test]
    async fn reader_detected_disconnect_promptly_reaps_the_writer_task() {
        let (client_io, server_io) = tokio::io::duplex(4096);
        let (client_read, client_write) = tokio::io::split(client_io);
        let (client, mut events) =
            CodexAppServerClient::connect_streams(BufReader::new(client_read), client_write);

        drop(server_io); // simulate the peer disconnecting - client never dropped

        let event = tokio::time::timeout(Duration::from_secs(5), events.recv()).await;
        assert!(
            matches!(event, Ok(Some(Event::Closed))),
            "expected Event::Closed promptly after peer disconnect, got {event:?}"
        );

        let reaped = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if matches!(client.send_initialized(), Err(Error::TransportClosed)) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        assert!(
            reaped.is_ok(),
            "writer task should be reaped shortly after the reader detects disconnect, \
             without needing the client itself to be dropped"
        );
    }

    /// Regression test: dropping every `CodexAppServerClient` clone must
    /// terminate the writer task (and, for a spawned child, kill it) even
    /// though the reader task independently holds its own `write_tx` clone
    /// for its whole lifetime. Verified indirectly via `connect_streams`: the
    /// writer task drops its write half on shutdown, which the peer observes
    /// as EOF.
    #[tokio::test]
    async fn dropping_the_last_client_terminates_the_writer_and_closes_the_transport() {
        let (client_io, mut server_io) = tokio::io::duplex(4096);
        let (client_read, client_write) = tokio::io::split(client_io);
        let (client, _events) =
            CodexAppServerClient::connect_streams(BufReader::new(client_read), client_write);

        drop(client);

        let mut buf = [0u8; 8];
        let result = tokio::time::timeout(Duration::from_secs(5), server_io.read(&mut buf)).await;
        assert!(
            matches!(result, Ok(Ok(0))),
            "expected EOF on the peer side after dropping the last client handle, got {result:?}"
        );
    }

    /// Regression test: for `connect_streams`/`connect_unix` (unlike
    /// `spawn()`, which can force the issue via `kill_on_drop`), dropping
    /// every `CodexAppServerClient` clone must terminate the reader task
    /// promptly even when the peer never sends EOF and never reacts to the
    /// writer's half-close. `_server_io` is kept alive but never read,
    /// written, or dropped - an uncooperative peer that will never produce
    /// EOF on its own. Before the reader task raced on `reader_cancel` (not
    /// just EOF/errors), this would hang forever instead of observing
    /// `Event::Closed`.
    #[tokio::test]
    async fn dropping_the_last_client_terminates_the_reader_even_without_peer_eof() {
        let (client_io, _server_io) = tokio::io::duplex(4096);
        let (client_read, client_write) = tokio::io::split(client_io);
        let (client, mut events) =
            CodexAppServerClient::connect_streams(BufReader::new(client_read), client_write);

        drop(client);

        let event = tokio::time::timeout(Duration::from_secs(5), events.recv()).await;
        assert!(
            matches!(event, Ok(Some(Event::Closed))),
            "expected the reader task to notice cancellation and emit Event::Closed \
             even without peer EOF, got {event:?}"
        );
    }

    /// Regression test: cancelling an in-flight call (here, via an outer
    /// `tokio::time::timeout` that drops the `call_request` future before it
    /// resolves) must not leak its entry in the pending-request map.
    #[tokio::test]
    async fn a_cancelled_call_does_not_leak_its_pending_map_entry() {
        let (client_io, _server_io) = tokio::io::duplex(4096);
        // `_server_io` is kept alive but never reads or writes, so no response
        // ever arrives and `call_request`'s own (much longer) timeout won't
        // fire either - only the outer cancellation below will.
        let (client_read, client_write) = tokio::io::split(client_io);
        let (client, _events) =
            CodexAppServerClient::connect_streams(BufReader::new(client_read), client_write);

        let result = tokio::time::timeout(
            Duration::from_millis(50),
            client.call_request(|id| ClientRequest::MemoryReset { id, params: () }),
        )
        .await;
        assert!(
            result.is_err(),
            "expected the outer timeout to cancel call_request before any response arrives"
        );
        assert!(
            client.pending.lock().unwrap().is_empty(),
            "pending map must be empty once the caller gave up on the call"
        );
    }
}
