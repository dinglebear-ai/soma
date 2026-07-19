//! Async-operation lifecycle: every mutation Incus documents as
//! long-running returns one of these, which callers wait on via
//! [`Client::wait_for_operation`] rather than assuming synchronous
//! completion.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::transport::{Client, IncusEnvelope, Method};

/// Defensive floor between re-issued long-poll calls when `timeout = None`.
/// Under Incus's documented behavior this branch should essentially never
/// fire - a no-timeout `.../wait` call blocks server-side until the
/// operation completes, so a genuinely quick non-terminal response
/// shouldn't happen - but if a daemon/proxy ever did return quickly and
/// repeatedly, this stops the loop from hot-spinning fresh Unix-socket
/// connections with no backoff at all.
const WAIT_REPOLL_MIN_INTERVAL: Duration = Duration::from_millis(50);

/// `#[non_exhaustive]`-equivalent: an unrecognized `class` value from a
/// future Incus version becomes `Other(<the raw string>)` rather than
/// failing deserialization outright (which is what a plain
/// `#[derive(Serialize, Deserialize)]` enum would do here) - consistent
/// with `Error`'s own `#[non_exhaustive]` forward-compatibility stance.
/// `Serialize`/`Deserialize` are implemented by hand rather than derived so
/// `Other` round-trips back to its original wire string exactly, instead of
/// serializing as `{"Other": "..."}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationClass {
    Task,
    Websocket,
    Token,
    Other(String),
}

impl OperationClass {
    fn as_wire_str(&self) -> &str {
        match self {
            OperationClass::Task => "task",
            OperationClass::Websocket => "websocket",
            OperationClass::Token => "token",
            OperationClass::Other(raw) => raw.as_str(),
        }
    }
}

impl Serialize for OperationClass {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_wire_str())
    }
}

impl<'de> Deserialize<'de> for OperationClass {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.as_str() {
            "task" => OperationClass::Task,
            "websocket" => OperationClass::Websocket,
            "token" => OperationClass::Token,
            _ => OperationClass::Other(raw),
        })
    }
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

/// Like [`operation_from_envelope`], but for endpoints Incus documents as
/// *conditionally* sync-or-async depending on the request payload rather
/// than always one or the other - e.g. creating a blank storage volume is
/// synchronous, but creating one by copying another volume is async
/// (verified against `cmd/incusd/storage_volumes.go`'s `doVolumeCreateOrCopy`
/// on the `lxc/incus` `main` branch: it returns `response.EmptySyncResponse`
/// when `req.Source.Name == ""`, and `operations.OperationResponse(op)`
/// otherwise). Returns `None` for a genuinely-sync response (nothing to
/// wait for) rather than trying to parse an `Operation` out of a body that
/// doesn't contain one - unlike `operation_from_envelope`, which assumes
/// every sync envelope it's given already contains the operation itself
/// (true for `wait_for_operation`'s `.../wait` responses, not true here).
pub(crate) fn optional_operation_from_envelope(
    envelope: IncusEnvelope,
) -> Result<Option<Operation>> {
    match envelope {
        IncusEnvelope::Sync { .. } => Ok(None),
        IncusEnvelope::Async { metadata } => Ok(Some(serde_json::from_value(metadata)?)),
    }
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
    pub async fn wait_for_operation(
        &self,
        id: Uuid,
        timeout: Option<Duration>,
    ) -> Result<Operation> {
        loop {
            let query_value;
            let query: &[(&str, &str)] = if let Some(duration) = timeout {
                // Round up rather than truncate: `duration.as_secs()` alone
                // would send `Some(Duration::from_millis(500))` as
                // `?timeout=0`, which - depending on how Incus treats a
                // zero-second wait - could return immediately instead of
                // honoring something close to the caller's actual request.
                let secs = duration.as_secs() + u64::from(duration.subsec_nanos() > 0);
                query_value = secs.to_string();
                &[("timeout", query_value.as_str())]
            } else {
                &[]
            };
            let path = format!("/1.0/operations/{id}/wait");
            // Bypass the client's default per-request timeout here - this
            // call already has its own server-side bound (the `timeout`
            // query param above, or a genuinely unbounded long-poll when
            // the caller passed `None`), and a legitimately slow operation
            // (e.g. a large image import) must not fail with a client-side
            // Error::Timeout just because it outlives the client's default,
            // which is sized for ordinary fast requests, not long-polls.
            let envelope = self
                .request_with_timeout(Method::Get, &path, query, None, None, None)
                .await?;
            let operation = operation_from_envelope(envelope)?;

            if !operation.is_terminal() {
                if timeout.is_some() {
                    // Caller set an explicit bound on one long-poll call;
                    // honor it rather than looping past it.
                    return Ok(operation);
                }
                // No caller-set bound: this window elapsed without a
                // terminal status, so re-issue the wait call and keep
                // waiting - after a small defensive floor (see
                // WAIT_REPOLL_MIN_INTERVAL's doc comment).
                tokio::time::sleep(WAIT_REPOLL_MIN_INTERVAL).await;
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
