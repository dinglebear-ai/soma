//! Optional bearer-token auth layer for the REST adapter.
//!
//! **This is transport auth only.** It answers exactly one question: did the
//! caller present the configured secret? It says nothing about
//! authorization (which sessions/methods a caller may touch), multi-tenant
//! isolation, or sandboxing of the underlying `codex app-server` process -
//! those remain entirely the host application's responsibility, same as
//! documented for the REST adapter as a whole in README.md. A caller that
//! presents the one shared token gets everything the mounted router exposes.
//!
//! ```rust,no_run
//! use codex_app_server_client::rest;
//!
//! # fn build() -> axum::Router {
//! rest::trusted_bridge_router().layer(rest::bearer_auth("super-secret-token"))
//! # }
//! ```

use std::{
    convert::Infallible,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use axum::{
    extract::Request,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Json, Response},
};
use tower_layer::Layer;
use tower_service::Service;

use super::types::RestErrorResponse;

/// Builds a [`BearerAuthLayer`] that requires `Authorization: Bearer <token>`
/// on every request except (by default) the health routes.
///
/// # Panics
///
/// Panics if `token` is empty or contains only whitespace. A blank
/// configured secret is never intentional - it is either a wiring bug (the
/// real token wasn't threaded through, e.g. an empty env var) or it would
/// accept a blank `Authorization: Bearer` header, which is functionally no
/// auth at all while looking like auth is configured. Failing fast at
/// construction is safer than accepting a broken configuration and starting
/// to accept blank tokens.
pub fn bearer_auth(token: impl Into<String>) -> BearerAuthLayer {
    BearerAuthLayer::new(token)
}

/// [`tower_layer::Layer`] that wraps a router (or any inner `tower` service)
/// with bearer-token auth.
///
/// Construct with [`bearer_auth`]. Use [`Self::allow_unauthenticated_health`]
/// to change whether `GET /health` and `GET /v1/health` require the token
/// too (they don't, by default - see that method's docs for why).
#[derive(Clone)]
pub struct BearerAuthLayer {
    token: Arc<[u8]>,
    allow_unauthenticated_health: bool,
}

impl BearerAuthLayer {
    fn new(token: impl Into<String>) -> Self {
        let token = token.into();
        assert!(
            !token.trim().is_empty(),
            "bearer_auth: configured token must not be empty or whitespace-only"
        );
        Self {
            token: token.into_bytes().into(),
            allow_unauthenticated_health: true,
        }
    }

    /// Controls whether `GET /health` and `GET /v1/health` (and *only* those
    /// two paths - not `GET /v1/compatibility`, which reveals the installed
    /// `codex` version) are reachable without a token.
    ///
    /// Defaults to `true`: liveness probes rarely carry credentials, and a
    /// bare "is the process up" response leaks nothing sensitive. Set this
    /// to `false` for deployments where even that is unwanted, e.g. an
    /// operator who wants every request authenticated uniformly regardless
    /// of what it reveals.
    pub fn allow_unauthenticated_health(mut self, allow: bool) -> Self {
        self.allow_unauthenticated_health = allow;
        self
    }
}

impl<S> Layer<S> for BearerAuthLayer {
    type Service = BearerAuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        BearerAuthService {
            inner,
            token: self.token.clone(),
            allow_unauthenticated_health: self.allow_unauthenticated_health,
        }
    }
}

/// The [`tower_service::Service`] produced by [`BearerAuthLayer`].
///
/// Public because it is the associated `Layer::Service` type of the public
/// `BearerAuthLayer` - not meant to be constructed directly, use
/// [`bearer_auth`].
#[derive(Clone)]
pub struct BearerAuthService<S> {
    inner: S,
    token: Arc<[u8]>,
    allow_unauthenticated_health: bool,
}

impl<S> Service<Request> for BearerAuthService<S>
where
    S: Service<Request, Response = Response, Error = Infallible> + Clone + Send + Sync + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Response, Infallible>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        // `Service::call` takes `&mut self`, but the returned future is boxed
        // and awaited independently of it, so the inner service has to be
        // moved into the future rather than borrowed.
        //
        // Swap rather than plain-clone: `poll_ready` above reserved capacity
        // on *`self.inner` specifically*, and tower's contract is that the
        // very instance which returned `Poll::Ready` is the one that must
        // receive the corresponding `call`. Handing the request to a fresh
        // clone would abandon that reservation and let the request skip the
        // inner service's backpressure. `mem::replace` moves the ready
        // instance out to be called and leaves the not-yet-ready clone behind
        // for the next `poll_ready`/`call` round-trip.
        //
        // This is invisible with an `axum::Router` underneath (its
        // `poll_ready` is always `Ready`), but `S` is generic and a caller may
        // well stack something load-shedding or rate-limiting below us.
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        let exempt = self.allow_unauthenticated_health && is_exempt_health_path(req.uri().path());
        if exempt || is_authorized(req.headers(), &self.token) {
            Box::pin(async move { inner.call(req).await })
        } else {
            Box::pin(async move { Ok(unauthorized_response()) })
        }
    }
}

fn is_exempt_health_path(path: &str) -> bool {
    matches!(path, "/health" | "/v1/health")
}

/// Checks the request's `Authorization` header against `expected`.
///
/// Always performs the same shape of work regardless of *why* the request
/// would be rejected (missing header, non-Bearer scheme, wrong token): it
/// extracts whatever token bytes are present (or an empty slice if none
/// are) and always runs [`constant_time_eq`] against `expected`. That keeps
/// "no header", "wrong scheme", and "right scheme, wrong token" from being
/// distinguishable by branch structure - only [`constant_time_eq`] itself
/// determines the answer.
fn is_authorized(headers: &HeaderMap, expected: &[u8]) -> bool {
    let provided = extract_bearer_token(headers).unwrap_or_default();
    constant_time_eq(provided.as_bytes(), expected)
}

/// Extracts the token from a case-insensitive `Authorization: Bearer
/// <token>` header. Returns `None` for a missing header, an unparsable
/// value, or any scheme other than `Bearer`.
fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| token.trim())
}

/// Constant-time byte comparison for the configured secret.
///
/// Deliberately not `==`: slice equality short-circuits on the first
/// differing byte (and on a length mismatch), which leaks timing
/// information an attacker can use to recover the secret byte-by-byte. This
/// compares every position up to `max(a.len(), b.len())` unconditionally -
/// there is no early return on the first difference and no early return on
/// a length mismatch (a length mismatch instead forces at least one
/// differing "byte" up front, folded into the same `|=` accumulator as
/// everything else) - so the amount of work done depends only on the
/// lengths involved, never on *where* (or whether) the inputs diverge.
///
/// No crypto crate is pulled in for this; it's a handful of lines and this
/// crate is deliberately dependency-minimal (see README.md).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff: u8 = u8::from(a.len() != b.len());
    let max_len = a.len().max(b.len());
    for index in 0..max_len {
        let left = a.get(index).copied().unwrap_or(0);
        let right = b.get(index).copied().unwrap_or(0);
        diff |= left ^ right;
    }
    diff == 0
}

fn unauthorized_response() -> Response {
    let mut response = (
        StatusCode::UNAUTHORIZED,
        Json(RestErrorResponse {
            error: "unauthorized".to_owned(),
            message: "a valid `Authorization: Bearer <token>` header is required".to_owned(),
            code: None,
            data: None,
        }),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_matches_equal_slices() {
        assert!(constant_time_eq(b"same-token", b"same-token"));
    }

    #[test]
    fn constant_time_eq_rejects_different_content_same_length() {
        assert!(!constant_time_eq(b"token-aaaa", b"token-bbbb"));
    }

    #[test]
    fn constant_time_eq_rejects_different_lengths() {
        assert!(!constant_time_eq(b"short", b"much-longer-token"));
        assert!(!constant_time_eq(b"much-longer-token", b"short"));
    }

    #[test]
    fn constant_time_eq_treats_two_empty_slices_as_equal() {
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn extract_bearer_token_is_case_insensitive_on_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("BeArEr abc123"),
        );
        assert_eq!(extract_bearer_token(&headers), Some("abc123"));
    }

    #[test]
    fn extract_bearer_token_rejects_other_schemes() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic abc123"),
        );
        assert_eq!(extract_bearer_token(&headers), None);
    }

    #[test]
    fn extract_bearer_token_rejects_missing_header() {
        assert_eq!(extract_bearer_token(&HeaderMap::new()), None);
    }

    #[test]
    #[should_panic(expected = "must not be empty or whitespace-only")]
    fn bearer_auth_panics_on_blank_token() {
        let _ = bearer_auth("   ");
    }

    #[test]
    #[should_panic(expected = "must not be empty or whitespace-only")]
    fn bearer_auth_panics_on_empty_token() {
        let _ = bearer_auth("");
    }
}
