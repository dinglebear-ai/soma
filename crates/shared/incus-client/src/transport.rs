//! The one place resource/operation code reaches the Incus daemon through:
//! [`Client::request`]. Everything below this module is HTTP framing
//! (`transport::unix`); everything above it (operations, resources) works
//! only with [`IncusEnvelope`], never raw bytes.

pub(crate) mod unix;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

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

/// Maps a 412 (Precondition Failed) API error - returned when a caller's
/// `If-Match` ETag was stale - into `Error::PreconditionFailed { resource }`.
/// Every other error passes through unchanged. Shared by every resource
/// module's `update_*`/`patch_*` methods that accept an `etag: Option<&str>`
/// precondition, so the 412-detection logic lives in exactly one place
/// rather than once per resource type.
pub(crate) fn precondition_failed_or(err: Error, resource: &str) -> Error {
    match err {
        Error::Api {
            status_code: 412, ..
        } => Error::PreconditionFailed {
            resource: resource.to_owned(),
        },
        other => other,
    }
}

/// Unwraps an [`IncusEnvelope::Sync`]'s metadata, or a distinguishing
/// `Error::InvalidResponse` if the envelope was some other shape. Shared by
/// every resource module's `list_*` method and the bare (non-ETag) `get_*`
/// methods (`get_storage_pool`, `get_storage_volume`) - the ETag-returning
/// `get_*` methods (`get_instance`, `get_image`, `get_network`,
/// `get_project`) match on `IncusEnvelope::Sync` directly instead, since they
/// also need the `etag` field to build a [`WithEtag`].
///
/// `what` names the expected response for the error message (e.g. `"list"`,
/// `"storage pool"`).
pub(crate) fn sync_metadata(envelope: IncusEnvelope, what: &str) -> Result<serde_json::Value> {
    match envelope {
        IncusEnvelope::Sync { metadata, .. } => Ok(metadata),
        other => Err(Error::InvalidResponse(format!(
            "expected a sync {what} response, got {other:?}"
        ))),
    }
}

/// Owns the string form of a `recursion` bool so a `[("recursion", &str)]`
/// query slice can be built without a dangling borrow - every `list_*`
/// method in this crate takes an explicit `recursion` bool and sends it as
/// this same query param.
pub(crate) struct RecursionQuery {
    value: String,
}

impl RecursionQuery {
    pub(crate) fn new(recursion: bool) -> Self {
        Self {
            value: recursion.to_string(),
        }
    }

    pub(crate) fn as_query(&self) -> [(&str, &str); 1] {
        [("recursion", self.value.as_str())]
    }
}

#[derive(Debug, Clone)]
struct ClientInner {
    socket_path: PathBuf,
    request_timeout: Option<Duration>,
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
            request_timeout: config.request_timeout,
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
            unix::RequestSpec {
                method,
                path,
                query,
                body: body_bytes.as_deref(),
                if_match,
            },
            self.0.request_timeout,
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

        let mut parsed: serde_json::Value = serde_json::from_slice(&raw.body)?;
        // Capture the envelope type as an owned String (one small clone) so
        // the immutable borrow of `parsed` ends here, freeing us to take
        // `metadata` out of `parsed` by value below via `.remove(...)`
        // instead of `.cloned()`-ing the whole (potentially large) subtree.
        let envelope_type = parsed
            .get("type")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                Error::InvalidResponse(format!("response body had no \"type\" field: {parsed}"))
            })?
            .to_owned();

        match envelope_type.as_str() {
            "sync" => {
                let metadata = parsed
                    .as_object_mut()
                    .and_then(|obj| obj.remove("metadata"))
                    .unwrap_or(serde_json::Value::Null);
                let etag = raw.header("etag").map(str::to_owned);
                Ok(IncusEnvelope::Sync { metadata, etag })
            }
            "async" => {
                // Validate envelope-shape strictness against the documented
                // Incus response even though the URL itself isn't stored -
                // `operation_from_envelope` derives the operation ID from
                // `metadata.id` instead, so this crate never needs to parse
                // it.
                if parsed
                    .get("operation")
                    .and_then(serde_json::Value::as_str)
                    .is_none()
                {
                    return Err(Error::InvalidResponse(
                        "async response had no \"operation\" field".to_owned(),
                    ));
                }
                let metadata = parsed
                    .as_object_mut()
                    .and_then(|obj| obj.remove("metadata"))
                    .ok_or_else(|| {
                        Error::InvalidResponse(
                            "async response had no \"metadata\" field".to_owned(),
                        )
                    })?;
                Ok(IncusEnvelope::Async { metadata })
            }
            other => Err(Error::InvalidResponse(format!(
                "unknown envelope type {other:?}"
            ))),
        }
    }

    /// Only called from `events::subscribe_events`, which is gated behind
    /// the `events` feature - suppress the dead-code lint under default
    /// features rather than widen this method's visibility or duplicate the
    /// socket path lookup in `events.rs`.
    #[cfg_attr(not(feature = "events"), allow(dead_code))]
    pub(crate) fn socket_path(&self) -> &std::path::Path {
        &self.0.socket_path
    }
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
