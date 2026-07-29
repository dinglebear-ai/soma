use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

use crate::{
    AllowAllApprovalHandler, ApprovalHandler, CodexAppServerClient, CodexSession,
    CompatibilityReport, DenyAllApprovalHandler, Error, Event, EventCollector,
    PendingServerRequest, ReadOnlyApprovalHandler, SessionOptions, TextTurnResult,
    protocol::{AskForApproval, SandboxMode, ThreadStartParams, TurnInterruptParams},
};

use super::types::{
    RestApprovalPolicy, RestBackend, RestCallRequest, RestCallResponse, RestClientOptions,
    RestError, RestErrorReplyRequest, RestEventResponse, RestFuture, RestLimits,
    RestRequestReplyResponse, RestRequestReplyResultRequest, RestResult, RestSessionCreateRequest,
    RestSessionCreateResponse, RestSessionSummary, RestStatusResponse, RestTextTurnRequest,
    RestTextTurnResponse, session_options_from,
};

/// Production REST backend.
///
/// One-shot calls create a short-lived Codex session. Stateful bridge calls use
/// sessions created by `POST /v1/sessions`.
#[derive(Clone)]
pub struct CodexRestBackend {
    sessions: Arc<CodexRestSessions>,
    limits: RestLimits,
    compatibility: Arc<StdMutex<Option<CachedCompatibility>>>,
    session_call_gate: Arc<Semaphore>,
}

impl Default for CodexRestBackend {
    fn default() -> Self {
        Self::with_limits(RestLimits::default())
    }
}

struct CodexRestSessions {
    sessions: Mutex<HashMap<String, Arc<CodexRestSession>>>,
    slots: Arc<Semaphore>,
    /// When [`CodexRestBackend::maybe_prune_idle_sessions`] last actually
    /// scanned. See [`PRUNE_MIN_INTERVAL`].
    last_prune: StdMutex<Instant>,
}

/// Floor on how often the *opportunistic* idle-session sweep on the hot path
/// ([`CodexRestBackend::session`]) actually scans.
///
/// That sweep used to run on every single `session()` call - i.e. on every
/// event poll and every session call - and each run is an O(sessions) scan
/// behind the global session lock. A single SSE stream draining a burst of
/// buffered events calls `session()` once per event, so the sweep's cost
/// scaled with event throughput while doing no useful work: sessions don't
/// become idle any faster because someone is polling a different one.
///
/// This only throttles the opportunistic sweep. The two callers whose
/// correctness depends on a fresh view - `create_session` (which sweeps to
/// reclaim a session slot before taking one) and `list_sessions` (which must
/// not report a session it is about to drop) - still sweep unconditionally,
/// so throttling here cannot make the backend hand out a stale slot or list a
/// dead session.
///
/// The cost of the throttle is that a session can outlive its
/// `idle_session_ttl` by up to this interval before hot-path traffic reaps
/// it. That is immaterial against the 30-minute default, and the interval is
/// capped by `idle_session_ttl` itself so a deliberately tiny TTL still
/// behaves as configured.
const PRUNE_MIN_INTERVAL: Duration = Duration::from_secs(1);

struct CodexRestSession {
    client: CodexAppServerClient,
    session: Mutex<CodexSession>,
    pending_requests: Mutex<HashMap<String, PendingRestRequest>>,
    last_used: StdMutex<Instant>,
    active_operations: AtomicUsize,
    call_gate: Arc<Semaphore>,
    _session_slot: OwnedSemaphorePermit,
}

struct SessionLease {
    session: Arc<CodexRestSession>,
}

struct PendingRestRequest {
    request: PendingServerRequest,
    expires_at: Instant,
}

struct CachedCompatibility {
    report: CompatibilityReport,
    expires_at: Instant,
}

impl CodexRestBackend {
    pub fn with_limits(limits: RestLimits) -> Self {
        let max_sessions = limits.max_sessions;
        let max_session_call_concurrency = limits.max_session_call_concurrency;
        Self {
            sessions: Arc::new(CodexRestSessions {
                sessions: Mutex::new(HashMap::new()),
                slots: Arc::new(Semaphore::new(max_sessions)),
                last_prune: StdMutex::new(Instant::now()),
            }),
            limits,
            compatibility: Arc::default(),
            session_call_gate: Arc::new(Semaphore::new(max_session_call_concurrency)),
        }
    }

    fn cached_compatibility(&self, now: Instant) -> Option<CompatibilityReport> {
        let cached = self
            .compatibility
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cached
            .as_ref()
            .filter(|cached| cached.expires_at > now)
            .map(|cached| cached.report.clone())
    }

    fn store_compatibility(&self, report: CompatibilityReport, now: Instant) {
        let mut cached = self
            .compatibility
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *cached = Some(CachedCompatibility {
            report,
            expires_at: now + self.limits.compatibility_ttl,
        });
    }

    async fn session(&self, session_id: &str) -> RestResult<SessionLease> {
        self.maybe_prune_idle_sessions().await;
        let session = self
            .sessions
            .sessions
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| RestError::NotFound(format!("session `{session_id}` was not found")))?;
        session.touch().await;
        session.active_operations.fetch_add(1, Ordering::AcqRel);
        Ok(SessionLease { session })
    }

    /// [`Self::prune_idle_sessions`], but at most once per
    /// [`PRUNE_MIN_INTERVAL`]. Use this on paths that sweep only
    /// opportunistically; use `prune_idle_sessions` directly where a fresh
    /// view is load-bearing.
    async fn maybe_prune_idle_sessions(&self) {
        let now = Instant::now();
        // The interval is capped by the TTL itself so that an operator who
        // deliberately sets a sub-second `idle_session_ttl` still gets
        // sweeping at the cadence they asked for rather than this floor.
        let interval = PRUNE_MIN_INTERVAL.min(self.limits.idle_session_ttl);
        {
            let mut last_prune = self
                .sessions
                .last_prune
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if now.duration_since(*last_prune) < interval {
                return;
            }
            // Claim the slot before releasing the lock and before doing the
            // scan, so concurrent callers arriving in the same instant don't
            // all decide to sweep.
            *last_prune = now;
        }
        self.prune_idle_sessions().await;
    }

    async fn prune_idle_sessions(&self) {
        let now = Instant::now();
        let mut sessions = self.sessions.sessions.lock().await;
        sessions.retain(|_, session| {
            let is_active = session.active_operations.load(Ordering::Acquire) > 0;
            let last_used = *session
                .last_used
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            is_active || now.duration_since(last_used) < self.limits.idle_session_ttl
        });
    }
}

/// Builds the [`SessionOptions`] used to spawn every session this backend
/// creates - one-shot text-turn sessions, stateful bridge sessions from
/// `POST /v1/sessions`, and session-less raw calls alike - threading
/// [`RestLimits::events_channel_capacity`] through via
/// [`SessionOptions::with_events_capacity`] so that configured limit
/// actually bounds the spawned session's event channel instead of every
/// session silently falling back to [`crate::DEFAULT_EVENTS_CHANNEL_CAPACITY`].
/// Centralized here (rather than inlined at each call site) so the three
/// `RestBackend` methods that spawn sessions can't drift out of sync with
/// this limit - see the unit test below for direct proof this actually
/// applies it.
fn session_options_with_limits(
    client: Option<RestClientOptions>,
    default_name: &str,
    limits: &RestLimits,
) -> SessionOptions {
    session_options_from(client, default_name).with_events_capacity(limits.events_channel_capacity)
}

fn rest_text_turn_thread_params(model: Option<String>) -> ThreadStartParams {
    let mut params = ThreadStartParams::new().ephemeral(true);
    params.model = model;
    params.approval_policy = Some(AskForApproval::Untrusted);
    params.sandbox = Some(SandboxMode::ReadOnly);
    params
}

async fn run_limited_text_turn<H>(
    session: &mut CodexSession,
    thread_params: ThreadStartParams,
    prompt: String,
    handler: &H,
    limits: &RestLimits,
) -> RestResult<TextTurnResult>
where
    H: ApprovalHandler,
{
    let thread = session.start_thread(thread_params).await?;
    let turn = session.send_text_turn(&thread.thread.id, prompt).await?;
    let thread_id = thread.thread.id.clone();
    let turn_id = turn.turn.id.clone();
    let collect_result = tokio::time::timeout(
        limits.max_text_turn_duration,
        collect_turn_with_output_limit(
            session,
            &thread_id,
            &turn_id,
            handler,
            limits.max_text_turn_output_bytes,
        ),
    )
    .await;
    let events = match collect_result {
        Ok(Ok(events)) => events,
        Ok(Err(error)) => return Err(error),
        Err(_elapsed) => {
            interrupt_turn(session, &thread_id, &turn_id).await;
            return Err(RestError::TimedOut(format!(
                "text turn did not complete within {:?}",
                limits.max_text_turn_duration
            )));
        }
    };
    Ok(TextTurnResult {
        thread,
        turn,
        events,
    })
}

async fn collect_turn_with_output_limit<H>(
    session: &mut CodexSession,
    thread_id: &str,
    turn_id: &str,
    handler: &H,
    max_output_bytes: usize,
) -> RestResult<EventCollector>
where
    H: ApprovalHandler,
{
    let mut collector = EventCollector::for_turn(thread_id, turn_id);
    while !collector.is_complete() {
        let Some(notification) = session.next_notification(handler).await else {
            return Err(RestError::Client(Error::TransportClosed));
        };
        collector.observe_notification(&notification);
        if collector.output_bytes() > max_output_bytes {
            interrupt_turn(session, thread_id, turn_id).await;
            return Err(RestError::PayloadTooLarge(format!(
                "text turn output exceeded {max_output_bytes} bytes"
            )));
        }
    }
    Ok(collector)
}

async fn interrupt_turn(session: &CodexSession, thread_id: &str, turn_id: &str) {
    let _ = session
        .client()
        .turn_interrupt(TurnInterruptParams {
            thread_id: thread_id.to_owned(),
            turn_id: turn_id.to_owned(),
        })
        .await;
}

impl CodexRestSession {
    async fn touch(&self) {
        *self
            .last_used
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Instant::now();
    }

    async fn prune_expired_pending(&self, now: Instant) {
        prune_expired_pending_requests(&self.pending_requests, now).await;
    }

    async fn take_pending_request(&self, request_key: &str) -> RestResult<PendingServerRequest> {
        take_pending_request(&self.pending_requests, request_key).await
    }
}

impl std::ops::Deref for SessionLease {
    type Target = CodexRestSession;

    fn deref(&self) -> &Self::Target {
        &self.session
    }
}

impl Drop for SessionLease {
    fn drop(&mut self) {
        self.session
            .active_operations
            .fetch_sub(1, Ordering::AcqRel);
        *self
            .session
            .last_used
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Instant::now();
    }
}

async fn prune_expired_pending_requests(
    pending_requests: &Mutex<HashMap<String, PendingRestRequest>>,
    now: Instant,
) {
    let mut pending = pending_requests.lock().await;
    let expired = pending
        .iter()
        .filter(|(_, request)| request.expires_at <= now)
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    for key in expired {
        pending.remove(&key);
    }
}

async fn take_pending_request(
    pending_requests: &Mutex<HashMap<String, PendingRestRequest>>,
    request_key: &str,
) -> RestResult<PendingServerRequest> {
    let now = Instant::now();
    let mut pending = pending_requests.lock().await;
    if pending
        .get(request_key)
        .is_some_and(|request| request.expires_at <= now)
    {
        pending.remove(request_key);
        return Err(RestError::Gone(format!(
            "request `{request_key}` has expired"
        )));
    }

    let expired = pending
        .iter()
        .filter(|(_, request)| request.expires_at <= now)
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    for key in expired {
        pending.remove(&key);
    }

    pending
        .remove(request_key)
        .map(|request| request.request)
        .ok_or_else(|| RestError::NotFound(format!("request `{request_key}` was not found")))
}

impl RestBackend for CodexRestBackend {
    fn compatibility_report(&self) -> RestFuture<CompatibilityReport> {
        let backend = self.clone();
        Box::pin(async move {
            let now = Instant::now();
            if let Some(report) = backend.cached_compatibility(now) {
                return Ok(report);
            }

            let report = tokio::task::spawn_blocking(CompatibilityReport::current)
                .await
                .map_err(|error| {
                    RestError::Internal(format!("compatibility check task failed: {error}"))
                })?;
            backend.store_compatibility(report.clone(), now);
            Ok(report)
        })
    }

    fn run_text_turn(&self, request: RestTextTurnRequest) -> RestFuture<RestTextTurnResponse> {
        let limits = self.limits.clone();
        Box::pin(async move {
            let session_options = session_options_with_limits(
                request.client.clone(),
                "codex_app_server_rest",
                &limits,
            );
            let approval_policy = request.approval_policy.unwrap_or_default();
            let thread_params = rest_text_turn_thread_params(request.model.clone());
            let prompt = request.prompt.clone();
            let mut session = CodexSession::spawn(session_options).await?;

            let result = match approval_policy {
                RestApprovalPolicy::DenyAll => {
                    run_limited_text_turn(
                        &mut session,
                        thread_params,
                        prompt,
                        &DenyAllApprovalHandler::default(),
                        &limits,
                    )
                    .await?
                }
                RestApprovalPolicy::ReadOnly => {
                    run_limited_text_turn(
                        &mut session,
                        thread_params,
                        prompt,
                        &ReadOnlyApprovalHandler,
                        &limits,
                    )
                    .await?
                }
                RestApprovalPolicy::AllowAll => {
                    run_limited_text_turn(
                        &mut session,
                        thread_params,
                        prompt,
                        &AllowAllApprovalHandler,
                        &limits,
                    )
                    .await?
                }
            };
            Ok(RestTextTurnResponse::from(result))
        })
    }

    fn create_session(
        &self,
        request: RestSessionCreateRequest,
    ) -> RestFuture<RestSessionCreateResponse> {
        let backend = self.clone();
        Box::pin(async move {
            backend.prune_idle_sessions().await;
            let session_slot =
                backend
                    .sessions
                    .slots
                    .clone()
                    .try_acquire_owned()
                    .map_err(|_| {
                        RestError::RateLimited(format!(
                            "maximum REST session count ({}) reached",
                            backend.limits.max_sessions
                        ))
                    })?;

            let session = CodexSession::spawn(session_options_with_limits(
                request.client,
                "codex_app_server_rest_session",
                &backend.limits,
            ))
            .await?;
            let initialize_response = serde_json::to_value(session.initialize_response())?;
            let client = session.client().clone();
            let session_id = format!("session-{}", Uuid::new_v4().simple());
            let rest_session = Arc::new(CodexRestSession {
                client,
                session: Mutex::new(session),
                pending_requests: Mutex::new(HashMap::new()),
                last_used: StdMutex::new(Instant::now()),
                active_operations: AtomicUsize::new(0),
                call_gate: Arc::new(Semaphore::new(
                    backend.limits.max_session_call_concurrency_per_session,
                )),
                _session_slot: session_slot,
            });
            let mut sessions = backend.sessions.sessions.lock().await;
            sessions.insert(session_id.clone(), rest_session);
            Ok(RestSessionCreateResponse {
                session_id,
                initialize_response,
            })
        })
    }

    fn list_sessions(&self) -> RestFuture<Vec<RestSessionSummary>> {
        let backend = self.clone();
        Box::pin(async move {
            backend.prune_idle_sessions().await;
            let mut sessions = backend
                .sessions
                .sessions
                .lock()
                .await
                .keys()
                .map(|session_id| RestSessionSummary {
                    session_id: session_id.clone(),
                })
                .collect::<Vec<_>>();
            sessions.sort_by(|left, right| left.session_id.cmp(&right.session_id));
            Ok(sessions)
        })
    }

    fn delete_session(&self, session_id: String) -> RestFuture<RestStatusResponse> {
        let backend = self.clone();
        Box::pin(async move {
            let removed = backend
                .sessions
                .sessions
                .lock()
                .await
                .remove(&session_id)
                .is_some();
            if removed {
                Ok(RestStatusResponse {
                    status: "deleted".to_owned(),
                })
            } else {
                Err(RestError::NotFound(format!(
                    "session `{session_id}` was not found"
                )))
            }
        })
    }

    fn call_method(&self, request: RestCallRequest) -> RestFuture<RestCallResponse> {
        let backend = self.clone();
        Box::pin(async move {
            let method = request.method.clone();
            let result = if let Some(session_id) = request.session_id.as_deref() {
                let session = backend.session(session_id).await?;
                let _global_permit = backend
                    .session_call_gate
                    .clone()
                    .try_acquire_owned()
                    .map_err(|_| {
                        RestError::RateLimited(format!(
                            "maximum REST session call concurrency ({}) reached",
                            backend.limits.max_session_call_concurrency
                        ))
                    })?;
                let _session_permit =
                    session.call_gate.clone().try_acquire_owned().map_err(|_| {
                        RestError::RateLimited(format!(
                            "maximum REST session call concurrency per session ({}) reached",
                            backend.limits.max_session_call_concurrency_per_session
                        ))
                    })?;
                session
                    .client
                    .call_raw_method(method.clone(), request.params)
                    .await?
            } else {
                let session = CodexSession::spawn(session_options_with_limits(
                    request.client,
                    "codex_app_server_rest_call",
                    &backend.limits,
                ))
                .await?;
                session
                    .client()
                    .call_raw_method(method.clone(), request.params)
                    .await?
            };
            Ok(RestCallResponse { method, result })
        })
    }

    fn poll_event(
        &self,
        session_id: String,
        timeout_ms: Option<u64>,
    ) -> RestFuture<RestEventResponse> {
        let limits = self.limits.clone();
        let backend = self.clone();
        Box::pin(async move {
            let session = backend.session(&session_id).await?;
            let timeout = Duration::from_millis(timeout_ms.unwrap_or(30_000));
            let event = tokio::time::timeout(timeout, async {
                let mut session_guard = session.session.session.lock().await;
                session_guard.next_event().await
            })
            .await;

            match event {
                Ok(Some(Event::Notification(notification))) => Ok(RestEventResponse::notification(
                    serde_json::to_value(notification)?,
                )),
                Ok(Some(Event::Request(request))) => {
                    let now = Instant::now();
                    session.prune_expired_pending(now).await;
                    let request_id = serde_json::to_value(request.id())?;
                    let method = request.method_name().to_owned();
                    let request_value = serde_json::to_value(&request.request)?;
                    let mut pending = session.pending_requests.lock().await;
                    if pending.len() >= limits.max_pending_requests_per_session {
                        let _ = request.respond_error(
                            -32000,
                            "REST pending request limit reached",
                            None,
                        );
                        return Err(RestError::RateLimited(format!(
                            "maximum pending request count ({}) reached",
                            limits.max_pending_requests_per_session
                        )));
                    }
                    let reply_deadline = request.reply_deadline();
                    if reply_deadline <= now {
                        let _ = request.respond_error(
                            -32000,
                            "REST request reply deadline already expired",
                            None,
                        );
                        return Err(RestError::Gone(
                            "server request can no longer be answered".to_owned(),
                        ));
                    }
                    let expires_at = (now + limits.pending_request_ttl).min(reply_deadline);
                    let request_key = format!("request-{}", Uuid::new_v4().simple());
                    pending.insert(
                        request_key.clone(),
                        PendingRestRequest {
                            request,
                            expires_at,
                        },
                    );
                    Ok(RestEventResponse::request(
                        request_key,
                        request_id,
                        method,
                        request_value,
                    ))
                }
                Ok(Some(Event::Closed)) | Ok(None) => Ok(RestEventResponse::closed()),
                Err(_elapsed) => Ok(RestEventResponse::timeout()),
            }
        })
    }

    fn reply_request_result(
        &self,
        session_id: String,
        request_key: String,
        body: RestRequestReplyResultRequest,
    ) -> RestFuture<RestRequestReplyResponse> {
        let backend = self.clone();
        Box::pin(async move {
            let session = backend.session(&session_id).await?;
            let pending = session.take_pending_request(&request_key).await?;
            pending.respond(body.result).map_err(|_| {
                RestError::Gone(format!("request `{request_key}` can no longer be answered"))
            })?;
            Ok(RestRequestReplyResponse {
                status: "ok".to_owned(),
            })
        })
    }

    fn reply_request_error(
        &self,
        session_id: String,
        request_key: String,
        body: RestErrorReplyRequest,
    ) -> RestFuture<RestRequestReplyResponse> {
        let backend = self.clone();
        Box::pin(async move {
            let session = backend.session(&session_id).await?;
            let pending = session.take_pending_request(&request_key).await?;
            pending
                .respond_error(body.code, body.message, body.data)
                .map_err(|_| {
                    RestError::Gone(format!("request `{request_key}` can no longer be answered"))
                })?;
            Ok(RestRequestReplyResponse {
                status: "ok".to_owned(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{CurrentTimeReadParams, RequestId, ServerRequest};

    /// The hot-path sweep must be throttled: `session()` runs on every event
    /// poll and every session call, and each unthrottled sweep is an
    /// O(sessions) scan behind the global session lock.
    ///
    /// Asserts against `last_prune` rather than a scan counter because the
    /// throttle's whole job is to *not* take the session lock, which leaves
    /// no other observable trace.
    #[tokio::test]
    async fn hot_path_sweep_is_throttled_but_correctness_paths_are_not() {
        let backend = CodexRestBackend::with_limits(RestLimits::default());
        let stamp = || {
            *backend
                .sessions
                .last_prune
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
        };

        // A fresh backend stamps `last_prune` at construction, so the first
        // hot-path call is inside the interval and must not sweep.
        let before = stamp();
        for _ in 0..64 {
            // `session()` on a missing id still runs the sweep before failing
            // the lookup, which is the path under test.
            let _ = backend.session("no-such-session").await;
        }
        assert_eq!(
            stamp(),
            before,
            "64 hot-path lookups inside the throttle interval should have swept zero times"
        );

        // Once the interval has elapsed, the next hot-path call sweeps again -
        // the throttle delays the sweep, it does not disable it.
        *backend
            .sessions
            .last_prune
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            Instant::now() - (PRUNE_MIN_INTERVAL * 2);
        let stale = stamp();
        let _ = backend.session("no-such-session").await;
        assert!(
            stamp() > stale,
            "a hot-path lookup after the interval elapsed should sweep"
        );

        // `list_sessions` must never report a session it is about to drop, so
        // it sweeps unconditionally regardless of when the last sweep ran.
        let just_swept = stamp();
        let sessions = backend.list_sessions().await.expect("list_sessions");
        assert!(sessions.is_empty());
        assert!(
            stamp() >= just_swept,
            "list_sessions must sweep unconditionally, not defer to the throttle"
        );
    }

    /// Regression test: `session_options_with_limits` is the single place
    /// all three `RestBackend` methods that spawn a session
    /// (`run_text_turn`, `create_session`, `call_method`'s session-less
    /// branch) go through to build `SessionOptions`. Exercises that exact
    /// production function directly with a non-default
    /// `events_channel_capacity` to prove the limit actually reaches the
    /// `SessionOptions` handed to `CodexSession::spawn` - a knob nothing
    /// reads is worse than no knob. (The complementary half of this proof -
    /// that a given `SessionOptions::events_capacity` actually bounds the
    /// resulting session's event channel - is covered by
    /// `client.rs`'s `connect_streams_with_events_capacity_bounds_the_channel_to_the_requested_size`.)
    #[test]
    fn session_options_with_limits_applies_the_configured_events_channel_capacity() {
        let limits = RestLimits {
            events_channel_capacity: 7,
            ..RestLimits::default()
        };
        assert_ne!(
            limits.events_channel_capacity,
            RestLimits::default().events_channel_capacity,
            "test setup bug: this must be a non-default value to prove anything"
        );

        let options = session_options_with_limits(None, "test-default-name", &limits);

        assert_eq!(options.events_capacity, Some(7));
    }

    #[tokio::test]
    async fn expired_pending_request_returns_gone_and_removes_the_key() {
        let pending_requests = Mutex::new(HashMap::from([(
            "request-1".to_owned(),
            PendingRestRequest {
                request: pending_current_time_request(),
                expires_at: Instant::now() - Duration::from_secs(1),
            },
        )]));

        let err = take_pending_request(&pending_requests, "request-1")
            .await
            .expect_err("expired request key should be rejected");

        assert!(matches!(err, RestError::Gone(_)));
        assert!(pending_requests.lock().await.is_empty());
    }

    #[tokio::test]
    async fn taking_pending_request_prunes_other_expired_keys() {
        let pending_requests = Mutex::new(HashMap::from([
            (
                "expired".to_owned(),
                PendingRestRequest {
                    request: pending_current_time_request(),
                    expires_at: Instant::now() - Duration::from_secs(1),
                },
            ),
            (
                "fresh".to_owned(),
                PendingRestRequest {
                    request: pending_current_time_request(),
                    expires_at: Instant::now() + Duration::from_secs(60),
                },
            ),
        ]));

        let request = take_pending_request(&pending_requests, "fresh")
            .await
            .expect("fresh request key should be returned");

        assert_eq!(request.method_name(), "currentTime/read");
        assert!(pending_requests.lock().await.is_empty());
    }

    fn pending_current_time_request() -> PendingServerRequest {
        PendingServerRequest::for_test(ServerRequest::CurrentTimeRead {
            id: RequestId::Int64(7),
            params: CurrentTimeReadParams {
                thread_id: "thread-test".to_owned(),
            },
        })
    }
}
