#![cfg(feature = "rest")]

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
};
use codex_app_server_client::{
    rest::{
        router_with_backend, router_with_backend_and_options, RestBackend, RestCallRequest,
        RestCallResponse, RestClientOptions, RestErrorReplyRequest, RestErrorResponse,
        RestEventResponse, RestHealthResponse, RestRequestReplyResponse,
        RestRequestReplyResultRequest, RestResult, RestRouterOptions, RestSessionCreateRequest,
        RestSessionCreateResponse, RestSessionSummary, RestStatusResponse, RestTextTurnRequest,
        RestTextTurnResponse,
    },
    CompatibilityReport, Error, SurfaceSummary, CODEX_SCHEMA_VERSION,
};
use serde_json::{json, Value};
use tower::ServiceExt;

#[derive(Clone, Default)]
struct FakeBackend {
    text_response: Arc<Mutex<Option<RestResult<RestTextTurnResponse>>>>,
    call_response: Arc<Mutex<Option<RestResult<RestCallResponse>>>>,
    session_response: Arc<Mutex<Option<RestResult<RestSessionCreateResponse>>>>,
    event_response: Arc<Mutex<Option<RestResult<RestEventResponse>>>>,
    observed_text: Arc<Mutex<Vec<RestTextTurnRequest>>>,
    observed_calls: Arc<Mutex<Vec<RestCallRequest>>>,
    observed_sessions: Arc<Mutex<Vec<RestSessionCreateRequest>>>,
    observed_polls: Arc<Mutex<Vec<Option<u64>>>>,
    observed_replies: Arc<Mutex<Vec<ObservedReply>>>,
    deleted_sessions: Arc<Mutex<Vec<String>>>,
}

impl FakeBackend {
    fn with_response(response: RestTextTurnResponse) -> Self {
        Self {
            text_response: Arc::new(Mutex::new(Some(Ok(response)))),
            ..Self::default()
        }
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

impl RestBackend for FakeBackend {
    fn compatibility_report(&self) -> CompatibilityReport {
        CompatibilityReport::from_installed_version(Some(CODEX_SCHEMA_VERSION.to_owned()))
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
        let response = self
            .event_response
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| Ok(RestEventResponse::timeout()));
        Box::pin(async move { response })
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
    let app = router_with_backend_and_options(backend, RestRouterOptions::trusted_bridge());

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
async fn rest_text_turn_rejects_malformed_json() {
    let app = router_with_backend(FakeBackend::default());
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
