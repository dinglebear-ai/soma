use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex as StdMutex,
    },
    time::{Duration, Instant},
};

use tokio::sync::Mutex;

use crate::{
    protocol::ThreadStartParams, AllowAllApprovalHandler, CodexAppServerClient, CodexSession,
    CompatibilityReport, DenyAllApprovalHandler, Event, PendingServerRequest,
    ReadOnlyApprovalHandler,
};

use super::types::{
    session_options_from, RestApprovalPolicy, RestBackend, RestCallRequest, RestCallResponse,
    RestError, RestErrorReplyRequest, RestEventResponse, RestFuture, RestLimits,
    RestRequestReplyResponse, RestRequestReplyResultRequest, RestResult, RestSessionCreateRequest,
    RestSessionCreateResponse, RestSessionSummary, RestStatusResponse, RestTextTurnRequest,
    RestTextTurnResponse,
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
}

impl Default for CodexRestBackend {
    fn default() -> Self {
        Self::with_limits(RestLimits::default())
    }
}

#[derive(Default)]
struct CodexRestSessions {
    next_session_id: AtomicU64,
    sessions: Mutex<HashMap<String, Arc<CodexRestSession>>>,
}

struct CodexRestSession {
    client: CodexAppServerClient,
    session: Mutex<CodexSession>,
    pending_requests: Mutex<HashMap<String, PendingRestRequest>>,
    next_request_key: AtomicU64,
    last_used: Mutex<Instant>,
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
        Self {
            sessions: Arc::default(),
            limits,
            compatibility: Arc::default(),
        }
    }

    async fn session(&self, session_id: &str) -> RestResult<Arc<CodexRestSession>> {
        self.prune_idle_sessions().await;
        let session = self
            .sessions
            .sessions
            .lock()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| RestError::NotFound(format!("session `{session_id}` was not found")))?;
        session.touch().await;
        Ok(session)
    }

    async fn prune_idle_sessions(&self) {
        let now = Instant::now();
        let entries = {
            let sessions = self.sessions.sessions.lock().await;
            sessions
                .iter()
                .map(|(id, session)| (id.clone(), session.clone()))
                .collect::<Vec<_>>()
        };

        let mut expired = Vec::new();
        for (id, session) in entries {
            let last_used = *session.last_used.lock().await;
            if now.duration_since(last_used) >= self.limits.idle_session_ttl {
                expired.push(id);
            }
        }

        if expired.is_empty() {
            return;
        }

        let mut sessions = self.sessions.sessions.lock().await;
        for id in expired {
            sessions.remove(&id);
        }
    }
}

impl CodexRestSession {
    async fn touch(&self) {
        *self.last_used.lock().await = Instant::now();
    }

    async fn prune_expired_pending(&self, now: Instant) {
        prune_expired_pending_requests(&self.pending_requests, now).await;
    }

    async fn take_pending_request(&self, request_key: &str) -> RestResult<PendingServerRequest> {
        take_pending_request(&self.pending_requests, request_key).await
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
    fn compatibility_report(&self) -> CompatibilityReport {
        let now = Instant::now();
        let mut cached = self
            .compatibility
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = cached.as_ref() {
            if cached.expires_at > now {
                return cached.report.clone();
            }
        }

        let report = CompatibilityReport::current();
        *cached = Some(CachedCompatibility {
            report: report.clone(),
            expires_at: now + self.limits.compatibility_ttl,
        });
        report
    }

    fn run_text_turn(&self, request: RestTextTurnRequest) -> RestFuture<RestTextTurnResponse> {
        Box::pin(async move {
            let session_options = request.session_options();
            let approval_policy = request.approval_policy.unwrap_or_default();
            let thread_params = request
                .model
                .clone()
                .map_or_else(ThreadStartParams::new, |model| {
                    ThreadStartParams::new().model(model)
                });
            let prompt = request.prompt.clone();
            let mut session = CodexSession::spawn(session_options).await?;

            let result = match approval_policy {
                RestApprovalPolicy::DenyAll => {
                    session
                        .run_text_turn_with_params_and_handler(
                            thread_params,
                            prompt,
                            &DenyAllApprovalHandler::default(),
                        )
                        .await?
                }
                RestApprovalPolicy::ReadOnly => {
                    session
                        .run_text_turn_with_params_and_handler(
                            thread_params,
                            prompt,
                            &ReadOnlyApprovalHandler,
                        )
                        .await?
                }
                RestApprovalPolicy::AllowAll => {
                    session
                        .run_text_turn_with_params_and_handler(
                            thread_params,
                            prompt,
                            &AllowAllApprovalHandler,
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
            if backend.sessions.sessions.lock().await.len() >= backend.limits.max_sessions {
                return Err(RestError::RateLimited(format!(
                    "maximum REST session count ({}) reached",
                    backend.limits.max_sessions
                )));
            }

            let session = CodexSession::spawn(session_options_from(
                request.client,
                "codex_app_server_rest_session",
            ))
            .await?;
            let initialize_response = serde_json::to_value(session.initialize_response())?;
            let client = session.client().clone();
            let session_id = format!(
                "session-{}",
                backend
                    .sessions
                    .next_session_id
                    .fetch_add(1, Ordering::Relaxed)
                    + 1
            );
            let rest_session = Arc::new(CodexRestSession {
                client,
                session: Mutex::new(session),
                pending_requests: Mutex::new(HashMap::new()),
                next_request_key: AtomicU64::new(0),
                last_used: Mutex::new(Instant::now()),
            });
            let mut sessions = backend.sessions.sessions.lock().await;
            if sessions.len() >= backend.limits.max_sessions {
                return Err(RestError::RateLimited(format!(
                    "maximum REST session count ({}) reached",
                    backend.limits.max_sessions
                )));
            }
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
                session
                    .client
                    .call_raw_method(method.clone(), request.params)
                    .await?
            } else {
                let session = CodexSession::spawn(session_options_from(
                    request.client,
                    "codex_app_server_rest_call",
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
                let mut session_guard = session.session.lock().await;
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
                    let request_key = format!(
                        "request-{}",
                        session.next_request_key.fetch_add(1, Ordering::Relaxed) + 1
                    );
                    let request_id = serde_json::to_value(request.id())?;
                    let method = request.method_name().to_owned();
                    let request_value = serde_json::to_value(&request.request)?;
                    let mut pending = session.pending_requests.lock().await;
                    if pending.len() >= limits.max_pending_requests_per_session {
                        request.respond_error(-32000, "REST pending request limit reached", None);
                        return Err(RestError::RateLimited(format!(
                            "maximum pending request count ({}) reached",
                            limits.max_pending_requests_per_session
                        )));
                    }
                    pending.insert(
                        request_key.clone(),
                        PendingRestRequest {
                            request,
                            expires_at: now + limits.pending_request_ttl,
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
            pending.respond(body.result)?;
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
            pending.respond_error(body.code, body.message, body.data);
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
