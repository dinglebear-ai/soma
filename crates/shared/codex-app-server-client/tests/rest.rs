#![cfg(feature = "rest")]

use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use codex_app_server_client::{
    CODEX_SCHEMA_VERSION, CompatibilityReport, Error, SurfaceSummary,
    rest::{
        RestApprovalPolicy, RestBackend, RestCallRequest, RestCallResponse, RestClientOptions,
        RestError, RestErrorReplyRequest, RestErrorResponse, RestEventResponse, RestHealthResponse,
        RestLimits, RestRequestReplyResponse, RestRequestReplyResultRequest, RestResult,
        RestRouterOptions, RestSessionCreateRequest, RestSessionCreateResponse, RestSessionSummary,
        RestStatusResponse, RestTextTurnRequest, RestTextTurnResponse, bearer_auth,
        router_with_backend, router_with_backend_and_options,
    },
};
use serde_json::{Value, json};
use tokio::sync::{Notify, watch};
use tower::ServiceExt;

#[derive(Clone, Default)]
struct FakeBackend {
    text_response: Arc<Mutex<Option<RestResult<RestTextTurnResponse>>>>,
    call_response: Arc<Mutex<Option<RestResult<RestCallResponse>>>>,
    session_response: Arc<Mutex<Option<RestResult<RestSessionCreateResponse>>>>,
    event_response: Arc<Mutex<Option<RestResult<RestEventResponse>>>>,
    /// Queue of canned `poll_event` responses consumed in order, one per
    /// call, before falling back to `event_response` / the default
    /// timeout. Lets tests script a sequence of events (e.g. for the SSE
    /// stream route) without juggling a single-shot `Option`.
    event_response_queue: Arc<Mutex<VecDeque<RestResult<RestEventResponse>>>>,
    observed_text: Arc<Mutex<Vec<RestTextTurnRequest>>>,
    observed_calls: Arc<Mutex<Vec<RestCallRequest>>>,
    observed_sessions: Arc<Mutex<Vec<RestSessionCreateRequest>>>,
    observed_polls: Arc<Mutex<Vec<Option<u64>>>>,
    observed_replies: Arc<Mutex<Vec<ObservedReply>>>,
    deleted_sessions: Arc<Mutex<Vec<String>>>,
    next_poll_block: Arc<Mutex<Option<PollBlock>>>,
}

impl FakeBackend {
    fn with_response(response: RestTextTurnResponse) -> Self {
        Self {
            text_response: Arc::new(Mutex::new(Some(Ok(response)))),
            ..Self::default()
        }
    }

    fn block_next_poll(&self) -> PollBlockHandle {
        let started = Arc::new(Notify::new());
        let (release, release_rx) = watch::channel(false);
        *self.next_poll_block.lock().unwrap() = Some(PollBlock {
            started: started.clone(),
            release: release_rx,
        });
        PollBlockHandle { started, release }
    }

    /// Queues a sequence of `poll_event` responses to be returned in order,
    /// one per call.
    fn queue_events(&self, responses: impl IntoIterator<Item = RestResult<RestEventResponse>>) {
        self.event_response_queue.lock().unwrap().extend(responses);
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ObservedReply {
    Result {
        session_id: String,
        request_key: String,
        body: RestRequestReplyResultRequest,
    },
    Error {
        session_id: String,
        request_key: String,
        body: RestErrorReplyRequest,
    },
}

struct PollBlock {
    started: Arc<Notify>,
    release: watch::Receiver<bool>,
}

struct PollBlockHandle {
    started: Arc<Notify>,
    release: watch::Sender<bool>,
}

impl RestBackend for FakeBackend {
    fn compatibility_report(
        &self,
    ) -> Pin<Box<dyn Future<Output = RestResult<CompatibilityReport>> + Send + 'static>> {
        Box::pin(async move {
            Ok(CompatibilityReport::from_installed_version(Some(
                CODEX_SCHEMA_VERSION.to_owned(),
            )))
        })
    }

    fn run_text_turn(
        &self,
        request: RestTextTurnRequest,
    ) -> Pin<Box<dyn Future<Output = RestResult<RestTextTurnResponse>> + Send + 'static>> {
        self.observed_text.lock().unwrap().push(request);
        let response = self
            .text_response
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| Ok(RestTextTurnResponse::default()));
        Box::pin(async move { response })
    }

    fn create_session(
        &self,
        request: RestSessionCreateRequest,
    ) -> Pin<Box<dyn Future<Output = RestResult<RestSessionCreateResponse>> + Send + 'static>> {
        self.observed_sessions.lock().unwrap().push(request);
        let response = self
            .session_response
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| {
                Ok(RestSessionCreateResponse {
                    session_id: "session-1".to_owned(),
                    initialize_response: json!({ "platformOs": "linux" }),
                })
            });
        Box::pin(async move { response })
    }

    fn list_sessions(
        &self,
    ) -> Pin<Box<dyn Future<Output = RestResult<Vec<RestSessionSummary>>> + Send + 'static>> {
        Box::pin(async move {
            Ok(vec![RestSessionSummary {
                session_id: "session-1".to_owned(),
            }])
        })
    }

    fn delete_session(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = RestResult<RestStatusResponse>> + Send + 'static>> {
        self.deleted_sessions.lock().unwrap().push(session_id);
        Box::pin(async move {
            Ok(RestStatusResponse {
                status: "deleted".to_owned(),
            })
        })
    }

    fn call_method(
        &self,
        request: RestCallRequest,
    ) -> Pin<Box<dyn Future<Output = RestResult<RestCallResponse>> + Send + 'static>> {
        self.observed_calls.lock().unwrap().push(request.clone());
        let response = self
            .call_response
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| {
                Ok(RestCallResponse {
                    method: request.method,
                    result: json!({ "ok": true }),
                })
            });
        Box::pin(async move { response })
    }

    fn poll_event(
        &self,
        _session_id: String,
        timeout_ms: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = RestResult<RestEventResponse>> + Send + 'static>> {
        self.observed_polls.lock().unwrap().push(timeout_ms);
        let block = self.next_poll_block.lock().unwrap().take();
        let queued = self.event_response_queue.lock().unwrap().pop_front();
        let response = queued.unwrap_or_else(|| {
            self.event_response
                .lock()
                .unwrap()
                .take()
                .unwrap_or_else(|| Ok(RestEventResponse::timeout()))
        });
        Box::pin(async move {
            if let Some(mut block) = block {
                block.started.notify_waiters();
                while !*block.release.borrow() {
                    if block.release.changed().await.is_err() {
                        break;
                    }
                }
            }
            response
        })
    }

    fn reply_request_result(
        &self,
        session_id: String,
        request_key: String,
        body: RestRequestReplyResultRequest,
    ) -> Pin<Box<dyn Future<Output = RestResult<RestRequestReplyResponse>> + Send + 'static>> {
        self.observed_replies
            .lock()
            .unwrap()
            .push(ObservedReply::Result {
                session_id,
                request_key,
                body,
            });
        Box::pin(async move {
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
    ) -> Pin<Box<dyn Future<Output = RestResult<RestRequestReplyResponse>> + Send + 'static>> {
        self.observed_replies
            .lock()
            .unwrap()
            .push(ObservedReply::Error {
                session_id,
                request_key,
                body,
            });
        Box::pin(async move {
            Ok(RestRequestReplyResponse {
                status: "ok".to_owned(),
            })
        })
    }
}

#[tokio::test]
async fn rest_router_exposes_health_and_compatibility() {
    let app = router_with_backend(FakeBackend::default());

    let (status, body) = request_json(app.clone(), Method::GET, "/health", None).await;
    assert_eq!(status, StatusCode::OK);
    let health: RestHealthResponse = serde_json::from_value(body).unwrap();
    assert_eq!(health.status, "ok");

    let (status, body) = request_json(app, Method::GET, "/v1/compatibility", None).await;
    assert_eq!(status, StatusCode::OK);
    let report: CompatibilityReport = serde_json::from_value(body).unwrap();
    assert_eq!(report.schema_codex_version, CODEX_SCHEMA_VERSION);
    assert_eq!(
        report.surface,
        SurfaceSummary {
            client_requests: 122,
            server_requests: 11,
            server_notifications: 68,
            client_notifications: 1,
        }
    );
}

#[tokio::test]
async fn rest_text_turn_maps_json_request_to_backend_response() {
    let backend = FakeBackend::with_response(RestTextTurnResponse {
        thread_id: "thread-1".to_owned(),
        turn_id: "turn-1".to_owned(),
        turn_status: Some("completed".to_owned()),
        agent_message: "hello over REST".to_owned(),
        latest_diff: Some("diff --git a/file b/file".to_owned()),
        errors: Vec::new(),
    });
    let observed = backend.observed_text.clone();
    let app = router_with_backend_and_options(
        backend,
        RestRouterOptions::text_turn().with_unsafe_client_options(true),
    );

    let body = json!({
        "prompt": "say hi",
        "model": "gpt-5",
        "approvalPolicy": "read_only",
        "client": {
            "name": "rest-test",
            "version": "0.1.0",
            "command": "codex-dev",
            "extraArgs": ["--experimental"],
            "config": {
                "model_reasoning_effort": "low"
            },
            "callTimeoutMs": 1500
        }
    });
    let (status, body) = request_json(app, Method::POST, "/v1/text-turn", Some(body)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["threadId"], "thread-1");
    assert_eq!(body["turnId"], "turn-1");
    assert_eq!(body["turnStatus"], "completed");
    assert_eq!(body["agentMessage"], "hello over REST");
    assert_eq!(body["latestDiff"], "diff --git a/file b/file");

    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert_eq!(
        observed[0],
        RestTextTurnRequest {
            prompt: "say hi".to_owned(),
            model: Some("gpt-5".to_owned()),
            approval_policy: Some(codex_app_server_client::rest::RestApprovalPolicy::ReadOnly),
            client: Some(RestClientOptions {
                name: Some("rest-test".to_owned()),
                version: Some("0.1.0".to_owned()),
                command: Some("codex-dev".to_owned()),
                extra_args: vec!["--experimental".to_owned()],
                config: [("model_reasoning_effort".to_owned(), "low".to_owned())].into(),
                call_timeout_ms: Some(1500),
            }),
        }
    );
}

#[tokio::test]
async fn default_router_rejects_unsafe_text_turn_controls() {
    let app = router_with_backend(FakeBackend::default());

    let (status, _body) = request_json(
        app,
        Method::POST,
        "/v1/text-turn",
        Some(json!({
            "prompt": "run something"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let app =
        router_with_backend_and_options(FakeBackend::default(), RestRouterOptions::text_turn());
    let (status, body) = request_json(
        app.clone(),
        Method::POST,
        "/v1/text-turn",
        Some(json!({
            "prompt": "run something",
            "approvalPolicy": "allow_all"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "forbidden");

    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/text-turn",
        Some(json!({
            "prompt": "run something",
            "client": {
                "command": "codex-dev",
                "extraArgs": ["--experimental"],
                "config": { "sandbox_mode": "danger-full-access" }
            }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "forbidden");
}

#[tokio::test]
async fn trusted_bridge_allows_allow_all_text_turn_and_forwards_policy() {
    let backend = FakeBackend::default();
    let observed = backend.observed_text.clone();
    let app = router_with_backend_and_options(
        backend,
        RestRouterOptions::trusted_bridge().with_unsafe_client_options(true),
    );

    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/text-turn",
        Some(json!({
            "prompt": "run trusted turn",
            "approvalPolicy": "allow_all"
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["threadId"], "");

    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert_eq!(
        observed[0].approval_policy,
        Some(RestApprovalPolicy::AllowAll)
    );
}

#[tokio::test]
async fn rest_text_turn_rejects_malformed_json() {
    let app =
        router_with_backend_and_options(FakeBackend::default(), RestRouterOptions::text_turn());
    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/text-turn")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{not json"))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: RestErrorResponse =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body.error, "invalid_json");
}

#[tokio::test]
async fn default_router_does_not_mount_raw_bridge_routes() {
    let app = router_with_backend(FakeBackend::default());
    let (status, _body) = request_json(
        app,
        Method::POST,
        "/v1/call/thread/start",
        Some(json!({ "params": { "model": "gpt-5" } })),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bridge_routes_reject_unsafe_client_options_when_not_trusted() {
    let backend = FakeBackend::default();
    let observed_calls = backend.observed_calls.clone();
    let observed_sessions = backend.observed_sessions.clone();
    let app = router_with_backend_and_options(
        backend,
        RestRouterOptions {
            enable_bridge_routes: true,
            allow_unsafe_client_options: false,
            ..RestRouterOptions::default()
        },
    );

    let (status, body) = request_json(
        app.clone(),
        Method::POST,
        "/v1/call/thread/start",
        Some(json!({
            "params": { "model": "gpt-5" },
            "client": { "command": "codex-dev" }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "forbidden");

    let (status, body) = request_json(
        app.clone(),
        Method::POST,
        "/v1/sessions",
        Some(json!({
            "client": {
                "extraArgs": ["--experimental"],
                "config": { "sandbox_mode": "danger-full-access" }
            }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "forbidden");

    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/sessions/session-1/call/turn/start",
        Some(json!({
            "params": { "threadId": "thread-1" },
            "client": { "config": { "model_provider": "custom" } }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_request");

    assert!(observed_calls.lock().unwrap().is_empty());
    assert!(observed_sessions.lock().unwrap().is_empty());
}

#[tokio::test]
async fn rest_raw_call_bridge_maps_method_path_params_and_client_options() {
    let backend = FakeBackend::default();
    *backend.call_response.lock().unwrap() = Some(Ok(RestCallResponse {
        method: "thread/start".to_owned(),
        result: json!({ "thread": { "id": "thread-1" } }),
    }));
    let observed = backend.observed_calls.clone();
    let app = router_with_backend_and_options(backend, RestRouterOptions::trusted_bridge());

    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/call/thread/start",
        Some(json!({
            "params": {
                "model": "gpt-5",
                "cwd": "/workspace"
            },
            "client": {
                "name": "bridge-test",
                "version": "0.1.0"
            }
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["method"], "thread/start");
    assert_eq!(body["result"]["thread"]["id"], "thread-1");

    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].session_id, None);
    assert_eq!(observed[0].method, "thread/start");
    assert_eq!(observed[0].params["model"], "gpt-5");
    assert_eq!(
        observed[0].client.as_ref().unwrap().name.as_deref(),
        Some("bridge-test")
    );
}

#[tokio::test]
async fn rest_raw_call_bridge_defaults_missing_params_to_null() {
    let backend = FakeBackend::default();
    let observed = backend.observed_calls.clone();
    let app = router_with_backend_and_options(backend, RestRouterOptions::trusted_bridge());

    let (status, body) =
        request_json(app, Method::POST, "/v1/call/memory/reset", Some(json!({}))).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["method"], "memory/reset");

    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].params, Value::Null);
}

#[tokio::test]
async fn rest_raw_call_bridge_preserves_json_rpc_error_details() {
    let backend = FakeBackend::default();
    *backend.call_response.lock().unwrap() = Some(Err(
        codex_app_server_client::rest::RestError::Client(Error::Rpc {
            code: -32602,
            message: "bad params".to_owned(),
            data: Some(json!({ "field": "params.cwd" })),
        }),
    ));
    let app = router_with_backend_and_options(backend, RestRouterOptions::trusted_bridge());

    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/call/thread/start",
        Some(json!({ "params": { "cwd": 1 } })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(body["error"], "json_rpc_error");
    assert_eq!(body["code"], -32602);
    assert_eq!(body["data"]["field"], "params.cwd");
}

#[tokio::test]
async fn rest_error_mapping_uses_expected_http_status_codes() {
    let cases = [
        (
            RestError::NotFound("missing session".to_owned()),
            StatusCode::NOT_FOUND,
            "not_found",
        ),
        (
            RestError::Gone("closed session".to_owned()),
            StatusCode::GONE,
            "gone",
        ),
        (
            RestError::Conflict("poll already active".to_owned()),
            StatusCode::CONFLICT,
            "conflict",
        ),
        (
            RestError::RateLimited("too many calls".to_owned()),
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
        ),
        (
            RestError::Client(Error::TransportClosed),
            StatusCode::BAD_GATEWAY,
            "codex_app_server_error",
        ),
    ];

    for (error, expected_status, expected_error) in cases {
        let backend = FakeBackend::default();
        *backend.call_response.lock().unwrap() = Some(Err(error));
        let app = router_with_backend_and_options(backend, RestRouterOptions::trusted_bridge());

        let (status, body) = request_json(
            app,
            Method::POST,
            "/v1/call/thread/start",
            Some(json!({ "params": { "model": "gpt-5" } })),
        )
        .await;

        assert_eq!(status, expected_status);
        assert_eq!(body["error"], expected_error);
    }
}

#[tokio::test]
async fn rest_stateful_bridge_creates_sessions_calls_methods_and_deletes_sessions() {
    let backend = FakeBackend::default();
    let observed_sessions = backend.observed_sessions.clone();
    let observed_calls = backend.observed_calls.clone();
    let deleted_sessions = backend.deleted_sessions.clone();
    let app = router_with_backend_and_options(backend, RestRouterOptions::trusted_bridge());

    let (status, body) = request_json(
        app.clone(),
        Method::POST,
        "/v1/sessions",
        Some(json!({
            "client": {
                "name": "session-test",
                "version": "0.1.0"
            }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["sessionId"], "session-1");
    assert_eq!(body["initializeResponse"]["platformOs"], "linux");

    let (status, body) = request_json(app.clone(), Method::GET, "/v1/sessions", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["sessions"][0]["sessionId"], "session-1");

    let (status, body) = request_json(
        app.clone(),
        Method::POST,
        "/v1/sessions/session-1/call/turn/start",
        Some(json!({
            "params": {
                "threadId": "thread-1",
                "input": [{ "type": "text", "text": "hello" }]
            }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["method"], "turn/start");
    assert_eq!(body["result"]["ok"], true);

    let (status, body) = request_json(app, Method::DELETE, "/v1/sessions/session-1", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "deleted");

    assert_eq!(
        observed_sessions.lock().unwrap()[0],
        RestSessionCreateRequest {
            client: Some(RestClientOptions {
                name: Some("session-test".to_owned()),
                version: Some("0.1.0".to_owned()),
                command: None,
                extra_args: Vec::new(),
                config: Default::default(),
                call_timeout_ms: None,
            }),
        }
    );
    assert_eq!(
        observed_calls.lock().unwrap()[0].session_id,
        Some("session-1".to_owned())
    );
    assert_eq!(observed_calls.lock().unwrap()[0].method, "turn/start");
    assert_eq!(deleted_sessions.lock().unwrap()[0], "session-1");
}

#[tokio::test]
async fn rest_stateful_bridge_exposes_events_and_request_replies() {
    let backend = FakeBackend::default();
    *backend.event_response.lock().unwrap() = Some(Ok(RestEventResponse::request(
        "pending-1",
        json!(42),
        "currentTime/read",
        json!({
            "id": 42,
            "method": "currentTime/read",
            "params": {
                "threadId": "thread-1"
            }
        }),
    )));
    let observed_replies = backend.observed_replies.clone();
    let app = router_with_backend_and_options(backend, RestRouterOptions::trusted_bridge());

    let (status, body) = request_json(
        app.clone(),
        Method::GET,
        "/v1/sessions/session-1/events?timeoutMs=1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["event"], "request");
    assert_eq!(body["requestKey"], "pending-1");
    assert_eq!(body["method"], "currentTime/read");
    assert_eq!(body["request"]["params"]["threadId"], "thread-1");

    let (status, body) = request_json(
        app.clone(),
        Method::POST,
        "/v1/sessions/session-1/requests/pending-1/result",
        Some(json!({ "result": { "currentTimeAt": 123 } })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");

    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/sessions/session-1/requests/pending-2/error",
        Some(json!({
            "code": -32000,
            "message": "denied",
            "data": { "reason": "test" }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");

    assert_eq!(
        *observed_replies.lock().unwrap(),
        vec![
            ObservedReply::Result {
                session_id: "session-1".to_owned(),
                request_key: "pending-1".to_owned(),
                body: RestRequestReplyResultRequest {
                    result: json!({ "currentTimeAt": 123 }),
                },
            },
            ObservedReply::Error {
                session_id: "session-1".to_owned(),
                request_key: "pending-2".to_owned(),
                body: RestErrorReplyRequest {
                    code: -32000,
                    message: "denied".to_owned(),
                    data: Some(json!({ "reason": "test" })),
                },
            },
        ]
    );
}

#[tokio::test]
async fn rest_router_enforces_configured_session_limit_before_backend_create() {
    let backend = FakeBackend::default();
    let observed_sessions = backend.observed_sessions.clone();
    let options = RestRouterOptions::trusted_bridge().with_max_sessions(0);
    let app = router_with_backend_and_options(backend, options);

    let (status, body) = request_json(app, Method::POST, "/v1/sessions", Some(json!({}))).await;

    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"], "rate_limited");
    assert!(observed_sessions.lock().unwrap().is_empty());
}

#[tokio::test]
async fn rest_router_clamps_event_poll_timeout() {
    let backend = FakeBackend::default();
    let observed_polls = backend.observed_polls.clone();
    let options = RestRouterOptions::trusted_bridge().with_max_poll_timeout_ms(50);
    let app = router_with_backend_and_options(backend, options);

    let (status, body) = request_json(
        app,
        Method::GET,
        "/v1/sessions/session-1/events?timeoutMs=5000",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["event"], "timeout");
    assert_eq!(observed_polls.lock().unwrap()[0], Some(50));
}

#[tokio::test]
async fn rest_router_rejects_second_simultaneous_poll_and_releases_guard() {
    let backend = FakeBackend::default();
    let poll_block = backend.block_next_poll();
    let poll_started = poll_block.started.notified();
    let observed_polls = backend.observed_polls.clone();
    let app = router_with_backend_and_options(backend, RestRouterOptions::trusted_bridge());

    let first_poll = tokio::spawn(request_json(
        app.clone(),
        Method::GET,
        "/v1/sessions/session-1/events?timeoutMs=5000",
        None,
    ));
    poll_started.await;

    let (status, body) = request_json(
        app.clone(),
        Method::GET,
        "/v1/sessions/session-1/events?timeoutMs=5000",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "conflict");

    poll_block.release.send(true).unwrap();
    let (status, body) = first_poll.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["event"], "timeout");

    let (status, body) = request_json(
        app,
        Method::GET,
        "/v1/sessions/session-1/events?timeoutMs=1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["event"], "timeout");
    assert_eq!(*observed_polls.lock().unwrap(), vec![Some(5000), Some(1)]);
}

#[tokio::test]
async fn sse_event_stream_is_absent_unless_bridge_routes_are_enabled() {
    let app =
        router_with_backend_and_options(FakeBackend::default(), RestRouterOptions::text_turn());

    let (status, _body) = request_json(
        app,
        Method::GET,
        "/v1/sessions/session-1/events/stream",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sse_event_stream_emits_events_in_order_and_terminates_on_closed() {
    let backend = FakeBackend::default();
    backend.queue_events([
        Ok(RestEventResponse::notification(
            json!({ "method": "turn/completed" }),
        )),
        Ok(RestEventResponse::request(
            "pending-1",
            json!(42),
            "currentTime/read",
            json!({
                "id": 42,
                "method": "currentTime/read",
                "params": { "threadId": "thread-1" }
            }),
        )),
        Ok(RestEventResponse::closed()),
    ]);
    let app = router_with_backend_and_options(backend, RestRouterOptions::trusted_bridge());

    let request = Request::builder()
        .method(Method::GET)
        .uri("/v1/sessions/session-1/events/stream")
        .body(Body::empty())
        .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(5), app.oneshot(request))
        .await
        .expect("SSE handler should return the response promptly")
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );

    // The stream only terminates once it forwards `Closed`, so this also
    // proves `Closed` ends the stream rather than hanging forever - the
    // outer `timeout` turns a regression there into a test failure instead
    // of a hang.
    let bytes = tokio::time::timeout(
        Duration::from_secs(5),
        to_bytes(response.into_body(), usize::MAX),
    )
    .await
    .expect("SSE stream should terminate after the Closed frame")
    .unwrap();
    let body = String::from_utf8(bytes.to_vec()).expect("SSE body should be UTF-8");
    let frames = parse_sse_frames(&body);

    assert_eq!(
        frames,
        vec![
            (
                "notification".to_owned(),
                json!({ "event": "notification", "notification": { "method": "turn/completed" } }),
            ),
            (
                "request".to_owned(),
                json!({
                    "event": "request",
                    "requestKey": "pending-1",
                    "requestId": 42,
                    "method": "currentTime/read",
                    "request": {
                        "id": 42,
                        "method": "currentTime/read",
                        "params": { "threadId": "thread-1" }
                    }
                }),
            ),
            ("closed".to_owned(), json!({ "event": "closed" })),
        ]
    );
}

#[tokio::test]
async fn sse_event_stream_rejects_concurrent_consumer_and_releases_guard_when_dropped() {
    let backend = FakeBackend::default();
    let app = router_with_backend_and_options(backend, RestRouterOptions::trusted_bridge());

    let stream_request = Request::builder()
        .method(Method::GET)
        .uri("/v1/sessions/session-1/events/stream")
        .body(Body::empty())
        .unwrap();
    let stream_response = app.clone().oneshot(stream_request).await.unwrap();
    assert_eq!(stream_response.status(), StatusCode::OK);

    // A plain long-poll for the same session is rejected while the SSE
    // stream is still alive - they share one `ActivePollGuard` per session.
    let (status, body) = request_json(
        app.clone(),
        Method::GET,
        "/v1/sessions/session-1/events?timeoutMs=1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "conflict");

    // Dropping the SSE response - simulating a client disconnect, since
    // nothing here ever polled the stream body to a `Closed` frame -
    // releases the guard.
    drop(stream_response);

    let (status, body) = request_json(
        app,
        Method::GET,
        "/v1/sessions/session-1/events?timeoutMs=1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["event"], "timeout");
}

fn auth_test_app() -> axum::Router {
    router_with_backend(FakeBackend::default()).layer(bearer_auth("secret-token"))
}

#[tokio::test]
async fn bearer_auth_rejects_missing_authorization_header() {
    let request = Request::builder()
        .method(Method::GET)
        .uri("/v1/compatibility")
        .body(Body::empty())
        .unwrap();
    let response = auth_test_app().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer")
    );
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: RestErrorResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body.error, "unauthorized");
}

#[tokio::test]
async fn bearer_auth_rejects_wrong_scheme() {
    let request = Request::builder()
        .method(Method::GET)
        .uri("/v1/compatibility")
        .header(header::AUTHORIZATION, "Basic secret-token")
        .body(Body::empty())
        .unwrap();
    let response = auth_test_app().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bearer_auth_rejects_wrong_token() {
    let request = Request::builder()
        .method(Method::GET)
        .uri("/v1/compatibility")
        .header(header::AUTHORIZATION, "Bearer wrong-token")
        .body(Body::empty())
        .unwrap();
    let response = auth_test_app().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bearer_auth_accepts_correct_token_with_case_insensitive_scheme() {
    let request = Request::builder()
        .method(Method::GET)
        .uri("/v1/compatibility")
        .header(header::AUTHORIZATION, "bEaReR secret-token")
        .body(Body::empty())
        .unwrap();
    let response = auth_test_app().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn bearer_auth_exempts_health_routes_by_default() {
    let app = auth_test_app();
    for path in ["/health", "/v1/health"] {
        let request = Request::builder()
            .method(Method::GET)
            .uri(path)
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "path {path} should be exempt from auth by default"
        );
    }
}

#[tokio::test]
async fn bearer_auth_does_not_exempt_compatibility_route() {
    let request = Request::builder()
        .method(Method::GET)
        .uri("/v1/compatibility")
        .body(Body::empty())
        .unwrap();
    let response = auth_test_app().oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bearer_auth_can_require_auth_on_health_routes_when_configured() {
    let app = router_with_backend(FakeBackend::default())
        .layer(bearer_auth("secret-token").allow_unauthenticated_health(false));
    let request = Request::builder()
        .method(Method::GET)
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// `std::env` is process-global; serializes the tests in this file that
/// mutate `CODEX_APP_SERVER_REST_*` variables against each other. Unit
/// tests in `src/rest/types.rs` use their own, separate lock - they run in
/// a different test binary/process, so there is no cross-binary race to
/// coordinate.
fn env_test_lock() -> &'static Mutex<()> {
    use std::sync::OnceLock;
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct EnvVarGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let previous = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(previous) => std::env::set_var(self.key, previous),
            None => std::env::remove_var(self.key),
        }
    }
}

#[tokio::test]
async fn rest_router_clamps_poll_timeout_using_env_derived_limits() {
    // The lock and the env var guard only need to be held while reading the
    // environment - once `limits` is built it's an owned, independent
    // value, so both are dropped before the `.await` below rather than held
    // across it (a `std::sync::MutexGuard` held across an await point is a
    // clippy lint, and there is nothing left for either guard to protect by
    // that point anyway).
    let limits = {
        let _lock = env_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _guard = EnvVarGuard::set("CODEX_APP_SERVER_REST_MAX_POLL_TIMEOUT_MS", "75");
        RestLimits::try_from_env().expect("a well-formed env override should parse cleanly")
    };
    assert_eq!(limits.max_poll_timeout, Duration::from_millis(75));

    let backend = FakeBackend::default();
    let observed_polls = backend.observed_polls.clone();
    let options = RestRouterOptions::trusted_bridge().with_limits(limits);
    let app = router_with_backend_and_options(backend, options);

    let (status, body) = request_json(
        app,
        Method::GET,
        "/v1/sessions/session-1/events?timeoutMs=5000",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["event"], "timeout");
    // The env-derived limit (75ms), not the requested 5000ms, reached the backend.
    assert_eq!(observed_polls.lock().unwrap()[0], Some(75));
}

/// Parses a raw SSE response body into `(event name, parsed JSON data)`
/// pairs, one per frame (frames are separated by a blank line). Only reads
/// the `event:` and `data:` fields - this crate's SSE frames never use
/// `id:`/`retry:`, and a bare keep-alive comment frame (`:...`) has no
/// `event:`/`data:` fields and would show up as a frame with neither, which
/// every assertion here intentionally does not expect to see (the fake
/// backend used in these tests always resolves well within the default
/// keep-alive interval).
fn parse_sse_frames(body: &str) -> Vec<(String, Value)> {
    body.split("\n\n")
        .map(str::trim)
        .filter(|frame| !frame.is_empty())
        .map(|frame| {
            let mut event_name = None;
            let mut data = None;
            for line in frame.lines() {
                if let Some(rest) = line.strip_prefix("event: ") {
                    event_name = Some(rest.to_owned());
                } else if let Some(rest) = line.strip_prefix("data: ") {
                    data = Some(
                        serde_json::from_str(rest).expect("SSE data field should be valid JSON"),
                    );
                }
            }
            (
                event_name.expect("frame should have an `event:` field"),
                data.expect("frame should have a `data:` field"),
            )
        })
        .collect()
}

async fn request_json(
    app: axum::Router,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    let request = if let Some(body) = body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        builder.body(Body::from(body.to_string())).unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

/// `?timeoutMs=0` on the streaming route must be clamped up to
/// [`RestLimits::min_stream_poll_timeout`], not passed through to the backend.
///
/// Without the floor, one request drives an unbounded run of back-to-back
/// `poll_event` calls (measured at ~935k frames/sec over a real socket), each
/// paying a session lock, an idle-session scan, an allocation, and a
/// serialized `timeout` frame to report that nothing happened.
#[tokio::test]
async fn sse_event_stream_clamps_a_zero_timeout_up_to_the_stream_floor() {
    let backend = FakeBackend::default();
    backend.queue_events([Ok(RestEventResponse::closed())]);
    let observed_polls = backend.observed_polls.clone();
    let options = RestRouterOptions::trusted_bridge();
    let floor_ms = options.limits.min_stream_poll_timeout.as_millis() as u64;
    let app = router_with_backend_and_options(backend, options);

    let request = Request::builder()
        .method(Method::GET)
        .uri("/v1/sessions/session-1/events/stream?timeoutMs=0")
        .body(Body::empty())
        .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(5), app.oneshot(request))
        .await
        .expect("SSE handler should return the response promptly")
        .unwrap();
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        to_bytes(response.into_body(), usize::MAX),
    )
    .await
    .expect("SSE stream should terminate after the Closed frame")
    .unwrap();

    assert_eq!(*observed_polls.lock().unwrap(), vec![Some(floor_ms)]);
}

/// The long-poll route must keep accepting `?timeoutMs=0` verbatim.
///
/// Pinned separately from the streaming floor above because the two routes
/// share `RestLimits` but not their clamping rules: `timeoutMs=0` is a
/// legitimate non-blocking "is anything waiting?" poll here, paced by one HTTP
/// round trip per call, and applying the stream's floor to it would silently
/// turn every such call into a 250ms block.
#[tokio::test]
async fn long_poll_route_still_accepts_a_zero_timeout_verbatim() {
    let backend = FakeBackend::default();
    let observed_polls = backend.observed_polls.clone();
    let app = router_with_backend_and_options(backend, RestRouterOptions::trusted_bridge());

    let (status, body) = request_json(
        app,
        Method::GET,
        "/v1/sessions/session-1/events?timeoutMs=0",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["event"], "timeout");
    assert_eq!(*observed_polls.lock().unwrap(), vec![Some(0)]);
}

/// A backend whose `poll_event` future resolves synchronously must not starve
/// the executor.
///
/// `RestBackend` is a public trait host applications implement, and resolving
/// on the first poll is legal and natural for a backend that already has an
/// event buffered - `FakeBackend` here does exactly that. Before
/// `EventPollStream` yielded on its own, such a backend meant `poll_next`
/// never returned `Poll::Pending`, so the task never handed control back to
/// the runtime and the stream looped forever with no upper bound.
///
/// This test would hang rather than fail without that fix: `#[tokio::test]`
/// runs a current-thread runtime, so a stream that never yields also prevents
/// the timer below from ever firing. The outer `timeout` is what converts a
/// regression into a failed assertion.
#[tokio::test]
async fn sse_event_stream_yields_to_the_executor_with_a_synchronous_backend() {
    // Default FakeBackend answers every poll with an immediate `timeout` and
    // never queues a terminal event, so this stream is infinite by design.
    let app = router_with_backend_and_options(
        FakeBackend::default(),
        RestRouterOptions::trusted_bridge(),
    );

    let request = Request::builder()
        .method(Method::GET)
        .uri("/v1/sessions/session-1/events/stream")
        .body(Body::empty())
        .unwrap();
    let response = tokio::time::timeout(Duration::from_secs(5), app.oneshot(request))
        .await
        .expect("SSE handler should return the response promptly")
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // Reading an infinite stream must time out, not hang: the timer firing at
    // all is the proof that the stream handed the runtime control back.
    let drained = tokio::time::timeout(
        Duration::from_millis(250),
        to_bytes(response.into_body(), usize::MAX),
    )
    .await;
    assert!(
        drained.is_err(),
        "an infinite SSE stream should still be pending when the timer fires; \
         if this resolved, the fake backend stopped being infinite"
    );
}

/// A request body over `RestLimits::max_request_body_bytes` is rejected with
/// `413` before the handler runs - and distinguishably from malformed JSON,
/// which is `400`. Guards the `DefaultBodyLimit` wiring plus `invalid_json`'s
/// status-passthrough for the too-large case.
#[tokio::test]
async fn oversized_request_body_is_rejected_with_413_not_400() {
    let options = RestRouterOptions::text_turn().with_limits(RestLimits {
        max_request_body_bytes: 64,
        ..RestLimits::default()
    });
    let app = router_with_backend_and_options(FakeBackend::default(), options);

    // Well over the 64-byte cap: 413, and the crate's payload_too_large shape.
    let big = json!({ "prompt": "x".repeat(500) });
    let (status, body) = request_json(app.clone(), Method::POST, "/v1/text-turn", Some(big)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body["error"], "payload_too_large");

    // Small but malformed body stays a 400 invalid_json - the cap must not
    // swallow the distinction between "too big" and "not JSON".
    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/text-turn")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{not json"))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"], "invalid_json");
}
