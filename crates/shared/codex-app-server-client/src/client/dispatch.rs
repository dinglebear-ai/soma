//! Parses one incoming NDJSON line from the app-server and routes it: resolve
//! a pending call, forward a server->client request (spawning a short-lived
//! task that tracks its reply/timeout), push a notification event, or warn
//! and drop the line if it doesn't parse into anything this crate's schema
//! understands. Split out from `client.rs` (the module this belongs to - see
//! the `super::` references throughout) purely to keep file sizes within
//! this repo's per-file budget; there is no architectural boundary between
//! the two beyond that.

use std::time::Instant;

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::{Event, PendingMap, PendingServerRequest, PENDING_SERVER_REQUEST_TIMEOUT};
use crate::protocol::{ServerNotification, ServerRequest};
use crate::transport::OutgoingReply;

/// Builds a raw JSON-RPC error response using `id` exactly as received (never
/// reinterpreted through [`crate::protocol::RequestId`]), so it round-trips
/// correctly whether the app-server used a string or integer id.
fn error_reply_line(id: &serde_json::Value, code: i64, message: String) -> Option<String> {
    let value = serde_json::json!({ "id": id, "error": { "code": code, "message": message, "data": null } });
    match serde_json::to_string(&value) {
        Ok(line) => Some(line),
        Err(err) => {
            // Only reachable if `id` (already-parsed JSON) somehow fails to
            // re-serialize, which shouldn't happen for a value that just
            // round-tripped through `serde_json::from_str`. Logged rather
            // than silently swallowed since every call site's whole purpose
            // is guaranteeing the app-server gets *some* reply.
            tracing::error!(error = %err, "failed to build a fallback JSON-RPC error reply line");
            None
        }
    }
}

/// Number of characters of a raw wire line included in diagnostic logs. This
/// is a truncation, not real redaction - JSON-RPC payloads on this protocol
/// can carry sensitive content (auth tokens, command output, arbitrary
/// agent-generated text), and these log sites fire on *undecodable* messages,
/// which are expected to happen occasionally in normal operation (e.g. a
/// newer app-server version using a method this crate's vendored schema
/// doesn't know about) - not just on truly exceptional/attacker-controlled
/// input. Keeping full lines out of logs by default bounds that exposure; a
/// short preview is still enough to identify which method/shape was involved
/// for debugging, since JSON-RPC messages put `method`/`id` up front.
const LOG_LINE_PREVIEW_CHARS: usize = 200;

fn line_preview(line: &str) -> String {
    if line.chars().count() <= LOG_LINE_PREVIEW_CHARS {
        line.to_string()
    } else {
        let preview: String = line.chars().take(LOG_LINE_PREVIEW_CHARS).collect();
        format!(
            "{preview}... ({} bytes total, truncated for logging)",
            line.len()
        )
    }
}

fn deliver_notification(events_tx: &mpsc::Sender<Event>, notification: ServerNotification) {
    let method = notification.method_name();
    match events_tx.try_send(Event::Notification(notification)) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(Event::Notification(notification)))
            if matches!(notification, ServerNotification::TurnCompleted(_)) =>
        {
            let events_tx = events_tx.clone();
            tokio::spawn(async move {
                if events_tx
                    .send(Event::Notification(notification))
                    .await
                    .is_err()
                {
                    tracing::warn!(
                        cause = "EventStream dropped",
                        method,
                        "dropping a terminal server notification"
                    );
                }
            });
        }
        Err(err) => {
            let cause = match err {
                mpsc::error::TrySendError::Full(_) => "channel full",
                mpsc::error::TrySendError::Closed(_) => "EventStream dropped",
            };
            tracing::warn!(cause, method, "dropping a server notification");
        }
    }
}

pub(super) fn dispatch_incoming_line(
    line: &str,
    pending: &PendingMap,
    events_tx: &mpsc::Sender<Event>,
    write_tx: &mpsc::Sender<String>,
    cancel: &CancellationToken,
) {
    let value: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(line_preview = %line_preview(line), error = %err, "app-server sent a line that is not valid JSON; ignoring");
            return;
        }
    };
    // Own the parsed object from here on (rather than borrowing via
    // `.as_object()`) so the response/error path below can `.remove()` the
    // `result`/`error` payload instead of cloning it - `result` in
    // particular can be an arbitrarily large value (e.g. a big `fs/readFile`
    // payload), and this is on the per-line hot path.
    let serde_json::Value::Object(mut map) = value else {
        tracing::warn!(line_preview = %line_preview(line), "app-server sent a non-object JSON-RPC line; ignoring");
        return;
    };

    if map.contains_key("method") {
        match map.get("id").cloned() {
            Some(id_value) => {
                let value = serde_json::Value::Object(map);
                match serde_json::from_value::<ServerRequest>(value) {
                    Ok(request) => {
                        let (reply_tx, reply_rx) = oneshot::channel::<OutgoingReply>();
                        let write_tx = write_tx.clone();
                        let timeout_id = id_value.clone();
                        let cancel = cancel.clone();
                        tokio::spawn(async move {
                            // Raced against the connection's own cancellation, not just
                            // PENDING_SERVER_REQUEST_TIMEOUT: without this, a caller sitting on
                            // a human-in-the-loop approval while the connection dies underneath
                            // them leaves this task lingering for up to 10 minutes doing
                            // nothing useful before its own timeout fires and no-ops on an
                            // already-dead write_tx.
                            tokio::select! {
                                _ = cancel.cancelled() => {
                                    // Connection already dead - nothing to write to and no one
                                    // left waiting for a reply; drop without a fallback write.
                                }
                                result = tokio::time::timeout(PENDING_SERVER_REQUEST_TIMEOUT, reply_rx) => {
                                    match result {
                                        Ok(Ok(reply)) => match reply.into_line() {
                                            Ok(line) => {
                                                let _ = write_tx.try_send(line);
                                            }
                                            Err(err) => {
                                                tracing::error!(
                                                    error = %err,
                                                    "failed to serialize a reply to a server->client \
                                                     request; the app-server will time out waiting for it"
                                                );
                                            }
                                        },
                                        Ok(Err(_recv_error)) => {
                                            // In practice unreachable via a normal drop:
                                            // `PendingServerRequest`'s own `Drop` impl always sends a
                                            // fallback reply before its `reply_tx` itself drops (see
                                            // that type's doc comment), so this only fires if a caller
                                            // bypasses `Drop` entirely (e.g. `std::mem::forget`) - kept
                                            // as a safe no-op rather than assuming it can't happen.
                                        }
                                        Err(_elapsed) => {
                                            tracing::warn!(
                                                timeout = ?PENDING_SERVER_REQUEST_TIMEOUT,
                                                "no one responded to a server->client request within the \
                                                 timeout; sending a fallback error reply instead of leaving \
                                                 the app-server waiting forever"
                                            );
                                            if let Some(line) = error_reply_line(
                                                &timeout_id,
                                                -32000,
                                                format!(
                                                    "codex-app-server-client: no reply within {PENDING_SERVER_REQUEST_TIMEOUT:?}"
                                                ),
                                            ) {
                                                let _ = write_tx.try_send(line);
                                            }
                                        }
                                    }
                                }
                            }
                        });
                        if let Err(err) = events_tx.try_send(Event::Request(PendingServerRequest {
                            request,
                            reply_tx: Some(reply_tx),
                            reply_deadline: Instant::now() + PENDING_SERVER_REQUEST_TIMEOUT,
                        })) {
                            // The consumer isn't draining EventStream fast enough (Full) or
                            // has dropped it entirely (Closed). `PendingServerRequest`'s own
                            // `Drop` impl would still send a generic fallback error if we let
                            // `psr` fall out of scope here, but replying explicitly gives the
                            // app-server a more specific, actionable message than that one.
                            let Event::Request(psr) = err.into_inner() else {
                                unreachable!("we always send Event::Request here")
                            };
                            tracing::warn!(
                                method = psr.method_name(),
                                "event channel unavailable (full or EventStream dropped); \
                                 replying with a fallback error instead of leaving this \
                                 server->client request unanswered"
                            );
                            let _ = psr.respond_error(
                                -32000,
                                "codex-app-server-client: event channel unavailable, request \
                                 could not be delivered",
                                None,
                            );
                        }
                    }
                    Err(err) => {
                        // The app-server expects a reply for every request it
                        // sends; if we can't understand this one (e.g. a
                        // method added in a newer app-server version than
                        // this crate's schema), silently dropping it leaves
                        // the app-server waiting forever. Fail it explicitly
                        // instead.
                        tracing::warn!(
                            line_preview = %line_preview(line), error = %err,
                            "could not decode a server->client request into any known method; \
                             sending back a JSON-RPC error instead of leaving the app-server waiting"
                        );
                        if let Some(reply) = error_reply_line(
                            &id_value,
                            -32601,
                            format!("codex-app-server-client: unrecognized or undecodable request: {err}"),
                        ) {
                            let _ = write_tx.try_send(reply);
                        } else {
                            tracing::error!(
                                "could not build even the fallback error reply; the app-server \
                                 will time out waiting for this request"
                            );
                        }
                    }
                }
            }
            None => {
                match serde_json::from_value::<ServerNotification>(serde_json::Value::Object(map)) {
                    Ok(notification) => {
                        // Fire-and-forget notifications are generally best-effort under
                        // backpressure, but a terminal turn event is the state transition
                        // `wait_for_turn_completed` depends on. Let that one wait for channel
                        // capacity in a tiny forwarding task instead of silently dropping it.
                        deliver_notification(events_tx, notification);
                    }
                    Err(err) => {
                        // Notifications never expect a reply, so there's nothing
                        // actionable to send back - just make the gap visible.
                        tracing::warn!(
                            line_preview = %line_preview(line), error = %err,
                            "could not decode a server notification into any known method; ignoring"
                        );
                    }
                }
            }
        }
        return;
    }

    let Some(id) = map.get("id").and_then(serde_json::Value::as_i64) else {
        // Either there's no "id" at all, or it's not an integer - we only
        // ever mint i64 request ids ourselves, so a non-integer id on a
        // response can't correlate to anything we're waiting on either way.
        tracing::warn!(line_preview = %line_preview(line), "app-server sent a response with no id, or an id we never issued; ignoring");
        return;
    };

    let sender = super::lock_pending(pending).remove(&id);
    let Some(sender) = sender else {
        tracing::debug!(
            id,
            "app-server response for an unknown or already-resolved request id"
        );
        return;
    };

    if let Some(result) = map.remove("result") {
        let _ = sender.send(Ok(result));
    } else if let Some(error) = map.remove("error") {
        let code = error
            .get("code")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(-1);
        let message = error
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let data = match error {
            serde_json::Value::Object(mut error_map) => error_map.remove("data"),
            _ => None,
        };
        let _ = sender.send(Err(crate::Error::Rpc {
            code,
            message,
            data,
        }));
    } else {
        tracing::warn!(line_preview = %line_preview(line), "app-server response has an id but neither result nor error");
    }
}
