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
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use crate::error::{Error, Result};
use crate::operations::Operation;
use crate::transport::Client;

/// Which event types to subscribe to. All `true` by default (subscribe to
/// everything Incus emits).
///
/// Setting every field to `false` sends `?type=` (an empty value) to Incus
/// rather than omitting the parameter - whether a real daemon then treats
/// that as "no filter, send everything" or "filter to nothing" is not
/// verified by this crate. [`EventFilter::default`] covers the common case;
/// if you deliberately construct an all-`false` filter, confirm the
/// behavior against a real daemon first.
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
///
/// This crate has no TLS transport in this epic (Incus's events endpoint is
/// reached over the same Unix socket as every other request), so the inner
/// stream is `WebSocketStream<UnixStream>` directly rather than wrapped in
/// `tokio_tungstenite::MaybeTlsStream`.
///
/// The hand-rolled `poll_next` below (rather than a `futures` combinator
/// chain like `filter_map`/`scan`) is a deliberate choice, not an
/// oversight: this stream needs three behaviors that don't compose cleanly
/// through a single combinator - skip-and-continue-polling (Ping/Pong),
/// emit-and-keep-going (a parsed event, or a recoverable per-frame error),
/// and emit-one-final-item-then-permanently-stop (an abnormal close).
/// `filter_map` alone can express the first two but conflates "skip" with
/// "stop", which is exactly the bug the `done` flag below fixes; layering
/// `scan` on top to add termination just moves the same state machine into
/// nested combinator closures without simplifying it.
pub struct EventStream {
    inner: WebSocketStream<UnixStream>,
    // Set once the connection has genuinely ended (a close frame was seen,
    // or the underlying stream itself ended). tokio-tungstenite doesn't
    // guarantee a `Close`/`None` observation is the *last* thing a poll
    // ever yields - without this, a caller could see the same
    // abnormal-close `Err` (or `None`) repeat indefinitely instead of the
    // stream actually terminating.
    done: bool,
}

impl Stream for EventStream {
    type Item = Result<Event>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        if self.done {
            return std::task::Poll::Ready(None);
        }
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
                // A close frame carrying a non-`Normal` code means the
                // daemon ended the subscription abnormally (restart,
                // internal error, protocol violation) rather than the
                // subscription simply running its course - surface that as
                // one `Err` item so a caller waiting on a specific event
                // can't mistake "the daemon dropped us" for "nothing more
                // to send."
                std::task::Poll::Ready(Some(Ok(Message::Close(Some(frame)))))
                    if frame.code != CloseCode::Normal =>
                {
                    self.done = true;
                    return std::task::Poll::Ready(Some(Err(Error::InvalidResponse(format!(
                        "/1.0/events subscription closed abnormally: {:?} ({})",
                        frame.code, frame.reason
                    )))));
                }
                std::task::Poll::Ready(Some(Ok(Message::Close(_))))
                | std::task::Poll::Ready(None) => {
                    self.done = true;
                    return std::task::Poll::Ready(None);
                }
                std::task::Poll::Ready(Some(Ok(Message::Binary(_) | Message::Frame(_)))) => {
                    return std::task::Poll::Ready(Some(Err(Error::InvalidResponse(
                        "unexpected binary websocket frame from /1.0/events".to_owned(),
                    ))));
                }
                // Distinguish a genuine socket I/O failure (`Transport`,
                // consistent with every other transport error in this
                // crate) from a WebSocket-layer protocol violation
                // (oversized frame, malformed handshake data, capacity
                // limit, ...) that isn't really an I/O problem at all - a
                // caller shouldn't have to parse the error message to tell
                // them apart.
                std::task::Poll::Ready(Some(Err(tokio_tungstenite::tungstenite::Error::Io(
                    io_err,
                )))) => {
                    return std::task::Poll::Ready(Some(Err(Error::Transport(io_err))));
                }
                std::task::Poll::Ready(Some(Err(err))) => {
                    return std::task::Poll::Ready(Some(Err(Error::WebSocketProtocol(
                        err.to_string(),
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
        let request = format!("ws://localhost/1.0/events?type={}", filter.query_value());
        let (ws_stream, _response) = tokio_tungstenite::client_async(request, stream)
            .await
            .map_err(|err| Error::Transport(std::io::Error::other(err.to_string())))?;
        Ok(EventStream {
            inner: ws_stream,
            done: false,
        })
    }
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
