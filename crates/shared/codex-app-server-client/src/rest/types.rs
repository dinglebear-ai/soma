use std::{collections::HashMap, env, future::Future, pin::Pin, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    CompatibilityReport, DEFAULT_EVENTS_CHANNEL_CAPACITY, Error, SessionOptions, TextTurnResult,
};

pub type RestResult<T> = std::result::Result<T, RestError>;
pub type RestFuture<T> = Pin<Box<dyn Future<Output = RestResult<T>> + Send + 'static>>;

/// REST router behavior knobs.
///
/// [`Default`] is intentionally non-executing: only health and compatibility are
/// mounted. Enable the text-turn helper or raw bridge routes explicitly when the
/// caller mounts the router behind its own authz boundary.
#[derive(Clone, Debug, Default)]
pub struct RestRouterOptions {
    pub enable_text_turn_route: bool,
    pub enable_bridge_routes: bool,
    pub allow_unsafe_client_options: bool,
    pub limits: RestLimits,
}

impl RestRouterOptions {
    /// Enables the one-shot text-turn helper without raw callable/session routes.
    pub fn text_turn() -> Self {
        Self {
            enable_text_turn_route: true,
            enable_bridge_routes: false,
            allow_unsafe_client_options: false,
            limits: RestLimits::default(),
        }
    }

    /// Enables the full raw callable bridge for trusted deployments.
    pub fn trusted_bridge() -> Self {
        Self {
            enable_text_turn_route: true,
            enable_bridge_routes: true,
            allow_unsafe_client_options: false,
            limits: RestLimits::default(),
        }
    }

    /// Allows request bodies to override the Codex command, extra arguments, and
    /// app-server config. This is intentionally separate from
    /// [`Self::trusted_bridge`]: admitting a caller to the bridge is not the same
    /// as allowing that caller to choose host executables or weaken sandboxing.
    pub fn with_unsafe_client_options(mut self, allow: bool) -> Self {
        self.allow_unsafe_client_options = allow;
        self
    }

    pub fn with_max_sessions(mut self, max_sessions: usize) -> Self {
        self.limits.max_sessions = max_sessions;
        self
    }

    pub fn with_max_poll_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.limits.max_poll_timeout = Duration::from_millis(timeout_ms);
        self
    }

    pub fn with_max_text_turn_duration_ms(mut self, timeout_ms: u64) -> Self {
        self.limits.max_text_turn_duration = Duration::from_millis(timeout_ms);
        self
    }

    pub fn with_max_text_turn_output_bytes(mut self, max_bytes: usize) -> Self {
        self.limits.max_text_turn_output_bytes = max_bytes;
        self
    }

    /// Sets the interval between SSE keep-alive frames on
    /// `GET /v1/sessions/{sessionId}/events/stream`. See
    /// [`RestLimits::sse_keep_alive_interval`].
    pub fn with_sse_keep_alive_interval_ms(mut self, interval_ms: u64) -> Self {
        self.limits.sse_keep_alive_interval = Duration::from_millis(interval_ms);
        self
    }

    /// Replaces the whole [`RestLimits`] set in one call, e.g. with
    /// [`RestLimits::from_env`] or [`RestLimits::try_from_env`]:
    ///
    /// ```rust,no_run
    /// # use codex_app_server_client::rest::{RestLimits, RestRouterOptions};
    /// let options = RestRouterOptions::trusted_bridge().with_limits(RestLimits::from_env());
    /// # let _ = options;
    /// ```
    pub fn with_limits(mut self, limits: RestLimits) -> Self {
        self.limits = limits;
        self
    }
}

/// Resource limits used by the default REST backend and route layer.
///
/// Every field has a hardcoded default (below) and can be overridden
/// independently via a `CODEX_APP_SERVER_REST_*` environment variable
/// through [`RestLimits::from_env`] / [`RestLimits::try_from_env`]. A
/// variable that is absent falls back to the field's default; a variable
/// that is *present but fails to parse* is a hard error
/// ([`RestLimitsEnvError`]), never a silent fallback - a malformed override
/// that quietly reverts to the default is exactly how an operator ships a
/// 10x-wrong limit and never notices.
///
/// | Field | Env var | Default |
/// |---|---|---|
/// | [`max_sessions`](Self::max_sessions) | `CODEX_APP_SERVER_REST_MAX_SESSIONS` | `16` |
/// | [`max_one_shot_concurrency`](Self::max_one_shot_concurrency) | `CODEX_APP_SERVER_REST_MAX_ONE_SHOT_CONCURRENCY` | `4` |
/// | [`max_session_call_concurrency`](Self::max_session_call_concurrency) | `CODEX_APP_SERVER_REST_MAX_SESSION_CALL_CONCURRENCY` | `64` |
/// | [`max_session_call_concurrency_per_session`](Self::max_session_call_concurrency_per_session) | `CODEX_APP_SERVER_REST_MAX_SESSION_CALL_CONCURRENCY_PER_SESSION` | `8` |
/// | [`max_poll_timeout`](Self::max_poll_timeout) | `CODEX_APP_SERVER_REST_MAX_POLL_TIMEOUT_MS` | `30000` (30s) |
/// | [`min_stream_poll_timeout`](Self::min_stream_poll_timeout) | `CODEX_APP_SERVER_REST_MIN_STREAM_POLL_TIMEOUT_MS` | `250` |
/// | [`max_text_turn_duration`](Self::max_text_turn_duration) | `CODEX_APP_SERVER_REST_MAX_TEXT_TURN_DURATION_MS` | `600000` (10m) |
/// | [`max_text_turn_output_bytes`](Self::max_text_turn_output_bytes) | `CODEX_APP_SERVER_REST_MAX_TEXT_TURN_OUTPUT_BYTES` | `1048576` (1 MiB) |
/// | [`max_request_body_bytes`](Self::max_request_body_bytes) | `CODEX_APP_SERVER_REST_MAX_REQUEST_BODY_BYTES` | `2097152` (2 MiB) |
/// | [`pending_request_ttl`](Self::pending_request_ttl) | `CODEX_APP_SERVER_REST_PENDING_REQUEST_TTL_MS` | `600000` (10m) |
/// | [`max_pending_requests_per_session`](Self::max_pending_requests_per_session) | `CODEX_APP_SERVER_REST_MAX_PENDING_REQUESTS_PER_SESSION` | `64` |
/// | [`events_channel_capacity`](Self::events_channel_capacity) | `CODEX_APP_SERVER_REST_EVENTS_CHANNEL_CAPACITY` | `1024` |
/// | [`idle_session_ttl`](Self::idle_session_ttl) | `CODEX_APP_SERVER_REST_IDLE_SESSION_TTL_MS` | `1800000` (30m) |
/// | [`compatibility_ttl`](Self::compatibility_ttl) | `CODEX_APP_SERVER_REST_COMPATIBILITY_TTL_MS` | `30000` (30s) |
/// | [`sse_keep_alive_interval`](Self::sse_keep_alive_interval) | `CODEX_APP_SERVER_REST_SSE_KEEP_ALIVE_MS` | `15000` (15s) |
#[derive(Clone, Debug)]
pub struct RestLimits {
    /// Maximum number of concurrently open stateful bridge sessions
    /// (`POST /v1/sessions`). Enforced both by a semaphore in
    /// [`crate::rest::CodexRestBackend`] and by an explicit pre-check in the
    /// route handler (so a full backend rejects before spawning a process).
    /// Env: `CODEX_APP_SERVER_REST_MAX_SESSIONS`. Default: `16`.
    pub max_sessions: usize,
    /// Maximum number of one-shot requests (`POST /v1/text-turn`,
    /// `POST /v1/call/{method}`) running at once; each spawns its own
    /// short-lived Codex process. Env:
    /// `CODEX_APP_SERVER_REST_MAX_ONE_SHOT_CONCURRENCY`. Default: `4`.
    pub max_one_shot_concurrency: usize,
    /// Maximum number of in-flight `POST /v1/sessions/{sessionId}/call/*`
    /// calls across *all* sessions combined. Env:
    /// `CODEX_APP_SERVER_REST_MAX_SESSION_CALL_CONCURRENCY`. Default: `64`.
    pub max_session_call_concurrency: usize,
    /// Maximum number of in-flight `POST /v1/sessions/{sessionId}/call/*`
    /// calls for a *single* session. Env:
    /// `CODEX_APP_SERVER_REST_MAX_SESSION_CALL_CONCURRENCY_PER_SESSION`.
    /// Default: `8`.
    pub max_session_call_concurrency_per_session: usize,
    /// Upper bound on `?timeoutMs=` for both
    /// `GET /v1/sessions/{sessionId}/events` and
    /// `GET /v1/sessions/{sessionId}/events/stream`; a larger requested
    /// value is clamped down to this. Env:
    /// `CODEX_APP_SERVER_REST_MAX_POLL_TIMEOUT_MS`. Default: `30000` (30s).
    pub max_poll_timeout: Duration,
    /// Lower bound on `?timeoutMs=` for `GET
    /// /v1/sessions/{sessionId}/events/stream` only; a smaller requested value
    /// is clamped up to this.
    ///
    /// The long-poll route has no floor on purpose (`timeoutMs=0` there is a
    /// legitimate "is anything waiting right now?" non-blocking poll, paced by
    /// one HTTP round trip per call). A stream has no such pacing and no use
    /// for a zero timeout, so without a floor one request can drive an
    /// unbounded run of back-to-back backend polls. Costs real events nothing:
    /// the timeout only bounds the *idle* wait, so this just caps how often an
    /// idle stream emits `timeout` frames. Env:
    /// `CODEX_APP_SERVER_REST_MIN_STREAM_POLL_TIMEOUT_MS`. Default: `250`.
    pub min_stream_poll_timeout: Duration,
    /// Wall-clock budget for `POST /v1/text-turn` to reach a terminal turn
    /// state; the turn is interrupted and the request fails with
    /// [`RestError::TimedOut`] past this point. Env:
    /// `CODEX_APP_SERVER_REST_MAX_TEXT_TURN_DURATION_MS`. Default:
    /// `600000` (10m).
    pub max_text_turn_duration: Duration,
    /// Byte cap on accumulated turn output for `POST /v1/text-turn`; the
    /// turn is interrupted and the request fails with
    /// [`RestError::PayloadTooLarge`] past this point. This is the
    /// response-byte-cap knob for the REST layer - the crate has no other
    /// hardcoded response size limit to promote (see the `rest`
    /// implementation notes for what was audited). Env:
    /// `CODEX_APP_SERVER_REST_MAX_TEXT_TURN_OUTPUT_BYTES`. Default:
    /// `1048576` (1 MiB).
    pub max_text_turn_output_bytes: usize,
    /// Cap on the size of a request *body*, applied to every route via axum's
    /// `DefaultBodyLimit`. A request whose body exceeds this is rejected with
    /// `413 Payload Too Large` before any handler runs.
    ///
    /// This is the input-side counterpart to
    /// [`Self::max_text_turn_output_bytes`]: without it, the only bound on an
    /// incoming prompt or raw-call params object is axum's own silent 2 MiB
    /// default, which is neither documented nor tunable. Making it an explicit
    /// `RestLimits` field keeps the "every limit has a default and an env
    /// override" contract true on the request side too. Env:
    /// `CODEX_APP_SERVER_REST_MAX_REQUEST_BODY_BYTES`. Default: `2097152`
    /// (2 MiB, matching axum's historical default).
    pub max_request_body_bytes: usize,
    /// How long a server-originated request surfaced by
    /// `GET /v1/sessions/{sessionId}/events(/stream)` stays answerable via
    /// `POST .../requests/{requestKey}/result` or `.../error` before it
    /// expires with [`RestError::Gone`] (also capped by the app-server's
    /// own reply deadline for that request, whichever is sooner). Env:
    /// `CODEX_APP_SERVER_REST_PENDING_REQUEST_TTL_MS`. Default: `600000`
    /// (10m).
    pub pending_request_ttl: Duration,
    /// Maximum number of not-yet-replied-to server requests a single
    /// session will hold at once; beyond this, new ones are rejected
    /// (with a JSON-RPC error sent back to the app-server on the caller's
    /// behalf) rather than buffered without bound. Env:
    /// `CODEX_APP_SERVER_REST_MAX_PENDING_REQUESTS_PER_SESSION`. Default:
    /// `64`.
    pub max_pending_requests_per_session: usize,
    /// Capacity of each REST-spawned session's internal event channel -
    /// exactly the channel documented on [`crate::EventStream`], applied via
    /// [`crate::SessionOptions::with_events_capacity`] to every session this
    /// backend spawns (`POST /v1/text-turn`, `POST /v1/sessions`, and
    /// session-less `POST /v1/call/{method}`). REST/SSE consumers reading
    /// events over a network are exactly the "slow or stalled consumer" case
    /// that channel's bound and drop policy exist for: notifications are
    /// dropped once it's full, but server-originated requests always get a
    /// fallback error reply first rather than being lost - see
    /// [`crate::EventStream`]'s doc comment for the exact per-variant policy.
    /// A `0` value is rejected when the session is actually spawned (see
    /// [`crate::CodexAppServerClient::spawn_with_events_capacity`]), not
    /// silently accepted here. Env:
    /// `CODEX_APP_SERVER_REST_EVENTS_CHANNEL_CAPACITY`. Default: `1024`
    /// (matching [`DEFAULT_EVENTS_CHANNEL_CAPACITY`]).
    pub events_channel_capacity: usize,
    /// How long a stateful bridge session may sit with no in-flight
    /// operation before it is pruned (and its `codex app-server` process
    /// torn down) on the next backend access. Env:
    /// `CODEX_APP_SERVER_REST_IDLE_SESSION_TTL_MS`. Default: `1800000`
    /// (30m).
    pub idle_session_ttl: Duration,
    /// How long a `GET /v1/compatibility` result is cached before the next
    /// call re-runs the (blocking, `codex --version`-invoking) check. Env:
    /// `CODEX_APP_SERVER_REST_COMPATIBILITY_TTL_MS`. Default: `30000`
    /// (30s).
    pub compatibility_ttl: Duration,
    /// Interval between SSE keep-alive frames sent by
    /// `GET /v1/sessions/{sessionId}/events/stream` while no real event is
    /// ready. Passed straight through to axum's
    /// [`KeepAlive::interval`](axum::response::sse::KeepAlive::interval).
    /// Env: `CODEX_APP_SERVER_REST_SSE_KEEP_ALIVE_MS`. Default: `15000`
    /// (15s, matching axum's own `KeepAlive` default - set explicitly here
    /// rather than relied upon, so this crate's behavior doesn't silently
    /// change if axum's default ever does).
    pub sse_keep_alive_interval: Duration,
}

impl Default for RestLimits {
    fn default() -> Self {
        Self {
            max_sessions: 16,
            max_one_shot_concurrency: 4,
            max_session_call_concurrency: 64,
            max_session_call_concurrency_per_session: 8,
            max_poll_timeout: Duration::from_secs(30),
            min_stream_poll_timeout: Duration::from_millis(250),
            max_text_turn_duration: Duration::from_secs(10 * 60),
            max_text_turn_output_bytes: 1024 * 1024,
            max_request_body_bytes: 2 * 1024 * 1024,
            pending_request_ttl: Duration::from_secs(600),
            max_pending_requests_per_session: 64,
            events_channel_capacity: DEFAULT_EVENTS_CHANNEL_CAPACITY,
            idle_session_ttl: Duration::from_secs(30 * 60),
            compatibility_ttl: Duration::from_secs(30),
            sse_keep_alive_interval: Duration::from_secs(15),
        }
    }
}

/// Error returned by [`RestLimits::try_from_env`] when a
/// `CODEX_APP_SERVER_REST_*` environment variable is set but cannot be
/// parsed as its expected type.
///
/// Deliberately distinct from [`RestError`]: this happens at process
/// startup, before any router or backend exists, so it can't be reported
/// through an HTTP response.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("environment variable `{var}` has an invalid value `{value}`: expected {expected}")]
pub struct RestLimitsEnvError {
    pub var: &'static str,
    pub value: String,
    pub expected: &'static str,
}

impl RestLimits {
    /// Builds [`RestLimits`] from `CODEX_APP_SERVER_REST_*` environment
    /// variables, using [`RestLimits::default`] for any variable that is
    /// absent.
    ///
    /// # Panics
    ///
    /// Panics (via [`RestLimitsEnvError`]'s `Display`) if any
    /// `CODEX_APP_SERVER_REST_*` variable is set but fails to parse. See
    /// [`RestLimits::try_from_env`] to handle that case without a panic -
    /// this constructor exists for the common case of one-shot process
    /// startup, where a malformed limit should abort startup loudly rather
    /// than be silently downgraded to the default or handled by caller
    /// code that has to remember to check.
    pub fn from_env() -> Self {
        match Self::try_from_env() {
            Ok(limits) => limits,
            Err(error) => panic!("{error}"),
        }
    }

    /// Builds [`RestLimits`] from `CODEX_APP_SERVER_REST_*` environment
    /// variables, using [`RestLimits::default`] for any variable that is
    /// absent, and returning [`RestLimitsEnvError`] for the first variable
    /// that is present but fails to parse.
    pub fn try_from_env() -> Result<Self, RestLimitsEnvError> {
        let default = Self::default();
        Ok(Self {
            max_sessions: env_usize("CODEX_APP_SERVER_REST_MAX_SESSIONS", default.max_sessions)?,
            max_one_shot_concurrency: env_usize(
                "CODEX_APP_SERVER_REST_MAX_ONE_SHOT_CONCURRENCY",
                default.max_one_shot_concurrency,
            )?,
            max_session_call_concurrency: env_usize(
                "CODEX_APP_SERVER_REST_MAX_SESSION_CALL_CONCURRENCY",
                default.max_session_call_concurrency,
            )?,
            max_session_call_concurrency_per_session: env_usize(
                "CODEX_APP_SERVER_REST_MAX_SESSION_CALL_CONCURRENCY_PER_SESSION",
                default.max_session_call_concurrency_per_session,
            )?,
            max_poll_timeout: env_duration_ms(
                "CODEX_APP_SERVER_REST_MAX_POLL_TIMEOUT_MS",
                default.max_poll_timeout,
            )?,
            min_stream_poll_timeout: env_duration_ms(
                "CODEX_APP_SERVER_REST_MIN_STREAM_POLL_TIMEOUT_MS",
                default.min_stream_poll_timeout,
            )?,
            max_text_turn_duration: env_duration_ms(
                "CODEX_APP_SERVER_REST_MAX_TEXT_TURN_DURATION_MS",
                default.max_text_turn_duration,
            )?,
            max_text_turn_output_bytes: env_usize(
                "CODEX_APP_SERVER_REST_MAX_TEXT_TURN_OUTPUT_BYTES",
                default.max_text_turn_output_bytes,
            )?,
            max_request_body_bytes: env_nonzero_usize(
                "CODEX_APP_SERVER_REST_MAX_REQUEST_BODY_BYTES",
                default.max_request_body_bytes,
            )?,
            pending_request_ttl: env_duration_ms(
                "CODEX_APP_SERVER_REST_PENDING_REQUEST_TTL_MS",
                default.pending_request_ttl,
            )?,
            max_pending_requests_per_session: env_usize(
                "CODEX_APP_SERVER_REST_MAX_PENDING_REQUESTS_PER_SESSION",
                default.max_pending_requests_per_session,
            )?,
            events_channel_capacity: env_nonzero_usize(
                "CODEX_APP_SERVER_REST_EVENTS_CHANNEL_CAPACITY",
                default.events_channel_capacity,
            )?,
            idle_session_ttl: env_duration_ms(
                "CODEX_APP_SERVER_REST_IDLE_SESSION_TTL_MS",
                default.idle_session_ttl,
            )?,
            compatibility_ttl: env_duration_ms(
                "CODEX_APP_SERVER_REST_COMPATIBILITY_TTL_MS",
                default.compatibility_ttl,
            )?,
            sse_keep_alive_interval: env_duration_ms(
                "CODEX_APP_SERVER_REST_SSE_KEEP_ALIVE_MS",
                default.sse_keep_alive_interval,
            )?,
        })
    }
}

/// Reads `var` as a `usize`, falling back to `default` only when the
/// variable is entirely absent. A variable that is set to anything that
/// doesn't parse as a `usize` (empty, negative, non-numeric, non-UTF-8) is
/// reported via `Err`, never silently mapped to `default`.
/// Like [`env_usize`], but rejects `0`.
///
/// For most limits `0` is merely a useless setting (a semaphore nobody can
/// acquire), which is the operator's business. For a channel capacity it is an
/// *invalid* one: `tokio::sync::mpsc::channel(0)` panics. Catching it here
/// means a bad value fails at `RestLimits::from_env()` - at startup, naming
/// the variable - instead of surviving the config load and blowing up on the
/// first session spawn, which is exactly the "present but unusable" case the
/// hard-error rule on [`RestLimits`] exists to prevent.
fn env_nonzero_usize(var: &'static str, default: usize) -> Result<usize, RestLimitsEnvError> {
    let value = env_usize(var, default)?;
    if value == 0 {
        return Err(RestLimitsEnvError {
            var,
            value: "0".to_owned(),
            expected: "a positive integer (a zero-capacity channel is not constructible)",
        });
    }
    Ok(value)
}

fn env_usize(var: &'static str, default: usize) -> Result<usize, RestLimitsEnvError> {
    match env::var(var) {
        Ok(value) => value
            .trim()
            .parse::<usize>()
            .map_err(|_| RestLimitsEnvError {
                var,
                value,
                expected: "a non-negative integer",
            }),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(env::VarError::NotUnicode(raw)) => Err(RestLimitsEnvError {
            var,
            value: raw.to_string_lossy().into_owned(),
            expected: "valid UTF-8 text",
        }),
    }
}

/// Reads `var` as a millisecond count and converts it to a [`Duration`],
/// falling back to `default` only when the variable is entirely absent. See
/// [`env_usize`] for the malformed-value policy (identical here).
fn env_duration_ms(var: &'static str, default: Duration) -> Result<Duration, RestLimitsEnvError> {
    match env::var(var) {
        Ok(value) => value
            .trim()
            .parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|_| RestLimitsEnvError {
                var,
                value,
                expected: "a non-negative integer count of milliseconds",
            }),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(env::VarError::NotUnicode(raw)) => Err(RestLimitsEnvError {
            var,
            value: raw.to_string_lossy().into_owned(),
            expected: "valid UTF-8 text",
        }),
    }
}

/// Errors surfaced by the optional REST adapter.
#[derive(Debug, thiserror::Error)]
pub enum RestError {
    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Gone(String),

    #[error("{0}")]
    Forbidden(String),

    #[error("{0}")]
    InvalidRequest(String),

    #[error("{0}")]
    RateLimited(String),

    #[error("{0}")]
    Conflict(String),

    #[error("{0}")]
    TimedOut(String),

    #[error("{0}")]
    PayloadTooLarge(String),

    #[error("{0}")]
    Internal(String),

    #[error(transparent)]
    Client(#[from] Error),
}

impl From<serde_json::Error> for RestError {
    fn from(error: serde_json::Error) -> Self {
        Self::Client(error.into())
    }
}

/// Backend used by the REST router.
///
/// The default [`crate::rest::CodexRestBackend`] talks to real `codex app-server`
/// processes. Tests and host applications can inject their own backend with
/// [`crate::rest::router_with_backend`] to control process lifecycle, pooling, or policy.
pub trait RestBackend: Send + Sync + 'static {
    fn compatibility_report(&self) -> RestFuture<CompatibilityReport>;
    fn run_text_turn(&self, request: RestTextTurnRequest) -> RestFuture<RestTextTurnResponse>;
    fn create_session(
        &self,
        _request: RestSessionCreateRequest,
    ) -> RestFuture<RestSessionCreateResponse> {
        Box::pin(async move {
            Err(RestError::NotFound(
                "REST session bridge is not implemented by this backend".to_owned(),
            ))
        })
    }
    fn list_sessions(&self) -> RestFuture<Vec<RestSessionSummary>> {
        Box::pin(async move { Ok(Vec::new()) })
    }
    fn delete_session(&self, session_id: String) -> RestFuture<RestStatusResponse> {
        Box::pin(async move {
            Err(RestError::NotFound(format!(
                "session `{session_id}` was not found"
            )))
        })
    }
    fn call_method(&self, request: RestCallRequest) -> RestFuture<RestCallResponse> {
        Box::pin(async move {
            Err(RestError::NotFound(format!(
                "REST raw call `{}` is not implemented by this backend",
                request.method
            )))
        })
    }
    fn poll_event(
        &self,
        session_id: String,
        timeout_ms: Option<u64>,
    ) -> RestFuture<RestEventResponse> {
        let _ = timeout_ms;
        Box::pin(async move {
            Err(RestError::NotFound(format!(
                "session `{session_id}` was not found"
            )))
        })
    }
    fn reply_request_result(
        &self,
        session_id: String,
        request_key: String,
        body: RestRequestReplyResultRequest,
    ) -> RestFuture<RestRequestReplyResponse> {
        let _ = (request_key, body);
        Box::pin(async move {
            Err(RestError::NotFound(format!(
                "session `{session_id}` was not found"
            )))
        })
    }
    fn reply_request_error(
        &self,
        session_id: String,
        request_key: String,
        body: RestErrorReplyRequest,
    ) -> RestFuture<RestRequestReplyResponse> {
        let _ = (request_key, body);
        Box::pin(async move {
            Err(RestError::NotFound(format!(
                "session `{session_id}` was not found"
            )))
        })
    }
}

/// Health response returned by `GET /health` and `GET /v1/health`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RestHealthResponse {
    pub status: String,
}

/// REST approval policy preset used while collecting turn events.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestApprovalPolicy {
    #[default]
    DenyAll,
    ReadOnly,
    AllowAll,
}

/// Optional client/session overrides for REST requests that spawn Codex.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestClientOptions {
    pub name: Option<String>,
    pub version: Option<String>,
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_args: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub config: HashMap<String, String>,
    pub call_timeout_ms: Option<u64>,
}

impl RestClientOptions {
    pub(super) fn into_session_options(self, default_name: &str) -> SessionOptions {
        let mut options = SessionOptions::new(
            self.name.unwrap_or_else(|| default_name.to_owned()),
            self.version
                .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned()),
        );
        if let Some(command) = self.command {
            options = options.with_command(command);
        }
        for arg in self.extra_args {
            options = options.with_extra_arg(arg);
        }
        for (key, value) in self.config {
            options = options.with_config(key, value);
        }
        if let Some(timeout_ms) = self.call_timeout_ms {
            options = options.with_call_timeout(Duration::from_millis(timeout_ms));
        }
        options
    }
}

/// Request body for `POST /v1/text-turn`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestTextTurnRequest {
    pub prompt: String,
    pub model: Option<String>,
    pub approval_policy: Option<RestApprovalPolicy>,
    pub client: Option<RestClientOptions>,
}

impl RestTextTurnRequest {
    pub fn session_options(&self) -> SessionOptions {
        session_options_from(self.client.clone(), "codex_app_server_rest")
    }
}

/// Response body for `POST /v1/text-turn`.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestTextTurnResponse {
    pub thread_id: String,
    pub turn_id: String,
    pub turn_status: Option<String>,
    pub agent_message: String,
    pub latest_diff: Option<String>,
    pub errors: Vec<Value>,
}

impl From<TextTurnResult> for RestTextTurnResponse {
    fn from(result: TextTurnResult) -> Self {
        let agent_message = result.agent_message().to_owned();
        let latest_diff = result.latest_diff().map(str::to_owned);
        let turn_status = result.events.terminal_status().and_then(|status| {
            serde_json::to_value(status)
                .ok()?
                .as_str()
                .map(str::to_owned)
        });
        let errors = result
            .errors()
            .iter()
            .map(|error| {
                serde_json::to_value(error).unwrap_or_else(|_| Value::String(error.message.clone()))
            })
            .collect();
        Self {
            thread_id: result.thread.thread.id,
            turn_id: result.turn.turn.id,
            turn_status,
            agent_message,
            latest_diff,
            errors,
        }
    }
}

/// Request body for raw method bridge calls.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestCallBody {
    #[serde(default)]
    pub params: Value,
    pub client: Option<RestClientOptions>,
}

/// Backend-facing raw method call request.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestCallRequest {
    pub session_id: Option<String>,
    pub method: String,
    pub params: Value,
    pub client: Option<RestClientOptions>,
}

/// Response body for raw method bridge calls.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestCallResponse {
    pub method: String,
    pub result: Value,
}

/// Request body for `POST /v1/sessions`.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestSessionCreateRequest {
    pub client: Option<RestClientOptions>,
}

/// Response body for `POST /v1/sessions`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestSessionCreateResponse {
    pub session_id: String,
    pub initialize_response: Value,
}

/// One session entry in `GET /v1/sessions`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestSessionSummary {
    pub session_id: String,
}

/// Response body for `GET /v1/sessions`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RestListSessionsResponse {
    pub sessions: Vec<RestSessionSummary>,
}

/// Simple status response used by mutating bridge endpoints.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RestStatusResponse {
    pub status: String,
}

/// Event returned by `GET /v1/sessions/{sessionId}/events`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(
    tag = "event",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RestEventResponse {
    Notification {
        notification: Value,
    },
    Request {
        request_key: String,
        request_id: Value,
        method: String,
        request: Value,
    },
    Closed,
    Timeout,
}

impl RestEventResponse {
    pub fn notification(notification: Value) -> Self {
        Self::Notification { notification }
    }

    pub fn request(
        request_key: impl Into<String>,
        request_id: Value,
        method: impl Into<String>,
        request: Value,
    ) -> Self {
        Self::Request {
            request_key: request_key.into(),
            request_id,
            method: method.into(),
            request,
        }
    }

    pub fn closed() -> Self {
        Self::Closed
    }

    pub fn timeout() -> Self {
        Self::Timeout
    }
}

/// Request body for replying successfully to a server-originated request.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestRequestReplyResultRequest {
    pub result: Value,
}

/// Request body for replying with an error to a server-originated request.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestErrorReplyRequest {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

/// Response body for request-reply endpoints.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RestRequestReplyResponse {
    pub status: String,
}

/// Structured REST error payload.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RestErrorResponse {
    pub error: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

pub(super) fn session_options_from(
    client: Option<RestClientOptions>,
    default_name: &str,
) -> SessionOptions {
    client
        .unwrap_or_default()
        .into_session_options(default_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        ffi::OsStr,
        sync::{Mutex, OnceLock},
    };

    /// `std::env` is process-global, so tests that mutate
    /// `CODEX_APP_SERVER_REST_*` variables must not run concurrently with
    /// each other (`cargo test` runs unit tests on multiple threads by
    /// default). This serializes just the tests in this module - it does
    /// not need to coordinate with anything outside this crate, since these
    /// variable names are only ever touched here and in the `rest`
    /// integration tests, which run in a separate test binary/process.
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: impl AsRef<OsStr>) -> Self {
            let previous = env::var_os(key);
            env::set_var(key, value);
            Self { key, previous }
        }

        fn unset(key: &'static str) -> Self {
            let previous = env::var_os(key);
            env::remove_var(key);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(previous) => env::set_var(self.key, previous),
                None => env::remove_var(self.key),
            }
        }
    }

    const ALL_REST_LIMIT_VARS: &[&str] = &[
        "CODEX_APP_SERVER_REST_MAX_SESSIONS",
        "CODEX_APP_SERVER_REST_MAX_ONE_SHOT_CONCURRENCY",
        "CODEX_APP_SERVER_REST_MAX_SESSION_CALL_CONCURRENCY",
        "CODEX_APP_SERVER_REST_MAX_SESSION_CALL_CONCURRENCY_PER_SESSION",
        "CODEX_APP_SERVER_REST_MAX_POLL_TIMEOUT_MS",
        "CODEX_APP_SERVER_REST_MAX_TEXT_TURN_DURATION_MS",
        "CODEX_APP_SERVER_REST_MAX_TEXT_TURN_OUTPUT_BYTES",
        "CODEX_APP_SERVER_REST_PENDING_REQUEST_TTL_MS",
        "CODEX_APP_SERVER_REST_MAX_PENDING_REQUESTS_PER_SESSION",
        "CODEX_APP_SERVER_REST_EVENTS_CHANNEL_CAPACITY",
        "CODEX_APP_SERVER_REST_IDLE_SESSION_TTL_MS",
        "CODEX_APP_SERVER_REST_COMPATIBILITY_TTL_MS",
        "CODEX_APP_SERVER_REST_SSE_KEEP_ALIVE_MS",
    ];

    /// Ensures every `CODEX_APP_SERVER_REST_*` variable starts (and ends)
    /// unset for a test, regardless of what the ambient environment
    /// happened to have, by unsetting (and restoring on drop) all of them.
    fn clear_all_rest_limit_vars() -> Vec<EnvVarGuard> {
        ALL_REST_LIMIT_VARS
            .iter()
            .map(|var| EnvVarGuard::unset(var))
            .collect()
    }

    #[test]
    fn try_from_env_uses_defaults_when_every_variable_is_absent() {
        let _lock = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _cleared = clear_all_rest_limit_vars();

        let limits = RestLimits::try_from_env().expect("all-absent env should use defaults");
        let default = RestLimits::default();
        assert_eq!(limits.max_sessions, default.max_sessions);
        assert_eq!(
            limits.max_session_call_concurrency_per_session,
            default.max_session_call_concurrency_per_session
        );
        assert_eq!(limits.max_poll_timeout, default.max_poll_timeout);
        assert_eq!(
            limits.events_channel_capacity,
            default.events_channel_capacity
        );
        assert_eq!(
            limits.sse_keep_alive_interval,
            default.sse_keep_alive_interval
        );
    }

    #[test]
    fn try_from_env_parses_valid_overrides() {
        let _lock = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _cleared = clear_all_rest_limit_vars();
        let _max_sessions = EnvVarGuard::set("CODEX_APP_SERVER_REST_MAX_SESSIONS", "42");
        let _poll_timeout = EnvVarGuard::set("CODEX_APP_SERVER_REST_MAX_POLL_TIMEOUT_MS", "5000");
        let _keep_alive = EnvVarGuard::set("CODEX_APP_SERVER_REST_SSE_KEEP_ALIVE_MS", "2500");

        let limits = RestLimits::try_from_env().expect("well-formed overrides should parse");

        assert_eq!(limits.max_sessions, 42);
        assert_eq!(limits.max_poll_timeout, Duration::from_millis(5000));
        assert_eq!(limits.sse_keep_alive_interval, Duration::from_millis(2500));
        // Every other field falls back to its default when unset.
        assert_eq!(
            limits.max_one_shot_concurrency,
            RestLimits::default().max_one_shot_concurrency
        );
    }

    #[test]
    fn try_from_env_reports_malformed_values_instead_of_defaulting() {
        let _lock = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _cleared = clear_all_rest_limit_vars();
        let _bad = EnvVarGuard::set("CODEX_APP_SERVER_REST_MAX_SESSIONS", "not-a-number");

        let error = RestLimits::try_from_env()
            .expect_err("a malformed override must not silently fall back to the default");

        assert_eq!(error.var, "CODEX_APP_SERVER_REST_MAX_SESSIONS");
        assert_eq!(error.value, "not-a-number");
    }

    #[test]
    fn try_from_env_reports_empty_string_as_malformed() {
        let _lock = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _cleared = clear_all_rest_limit_vars();
        let _bad = EnvVarGuard::set("CODEX_APP_SERVER_REST_MAX_POLL_TIMEOUT_MS", "");

        let error = RestLimits::try_from_env()
            .expect_err("an empty override must not silently fall back to the default");

        assert_eq!(error.var, "CODEX_APP_SERVER_REST_MAX_POLL_TIMEOUT_MS");
    }

    #[test]
    fn try_from_env_reports_negative_values_as_malformed() {
        let _lock = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _cleared = clear_all_rest_limit_vars();
        let _bad = EnvVarGuard::set("CODEX_APP_SERVER_REST_MAX_SESSIONS", "-1");

        let error = RestLimits::try_from_env()
            .expect_err("a negative override must not silently fall back to the default");

        assert_eq!(error.var, "CODEX_APP_SERVER_REST_MAX_SESSIONS");
    }

    /// A zero events capacity has to fail here, at config load, rather than
    /// parsing cleanly and panicking inside `tokio::sync::mpsc::channel` the
    /// first time a session is spawned - by which point the process has
    /// already printed a startup banner claiming the configuration is good.
    #[test]
    fn try_from_env_rejects_a_zero_events_channel_capacity_at_load_time() {
        let _lock = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _cleared = clear_all_rest_limit_vars();
        let _bad = EnvVarGuard::set("CODEX_APP_SERVER_REST_EVENTS_CHANNEL_CAPACITY", "0");

        let error = RestLimits::try_from_env()
            .expect_err("a zero events channel capacity is not constructible and must be rejected");

        assert_eq!(error.var, "CODEX_APP_SERVER_REST_EVENTS_CHANNEL_CAPACITY");
        assert!(
            error.to_string().contains("positive"),
            "the error should say what was expected, got: {error}"
        );
    }

    #[test]
    fn try_from_env_parses_a_valid_events_channel_capacity_override() {
        let _lock = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _cleared = clear_all_rest_limit_vars();
        let _capacity = EnvVarGuard::set("CODEX_APP_SERVER_REST_EVENTS_CHANNEL_CAPACITY", "7");

        let limits = RestLimits::try_from_env().expect("a well-formed override should parse");

        assert_eq!(limits.events_channel_capacity, 7);
    }

    #[test]
    fn try_from_env_reports_a_malformed_events_channel_capacity_instead_of_defaulting() {
        let _lock = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _cleared = clear_all_rest_limit_vars();
        let _bad = EnvVarGuard::set(
            "CODEX_APP_SERVER_REST_EVENTS_CHANNEL_CAPACITY",
            "not-a-number",
        );

        let error = RestLimits::try_from_env()
            .expect_err("a malformed override must not silently fall back to the default");

        assert_eq!(error.var, "CODEX_APP_SERVER_REST_EVENTS_CHANNEL_CAPACITY");
        assert_eq!(error.value, "not-a-number");
    }

    #[test]
    #[should_panic(expected = "CODEX_APP_SERVER_REST_MAX_SESSIONS")]
    fn from_env_panics_on_malformed_override() {
        let _lock = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _cleared = clear_all_rest_limit_vars();
        let _bad = EnvVarGuard::set("CODEX_APP_SERVER_REST_MAX_SESSIONS", "nope");

        let _ = RestLimits::from_env();
    }
}
