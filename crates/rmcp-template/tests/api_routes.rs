//! Route-level tests for REST dispatch, status, and mounted auth behavior.
#![cfg(feature = "mcp-http")]

use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
};
use rmcp_template::{
    api::rest_routes,
    server::{self, AuthPolicy},
    testing::{bearer_state, loopback_state},
};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn request_json(
    app: axum::Router,
    method: Method,
    path: &str,
    auth: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = auth {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let request = if let Some(body) = body {
        builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .expect("request should build")
    } else {
        builder.body(Body::empty()).expect("request should build")
    };

    let response = app.oneshot(request).await.expect("route should respond");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should read");
    let value = serde_json::from_slice(&bytes).expect("response should be JSON");
    (status, value)
}

#[tokio::test]
async fn direct_rest_echo_accepts_typed_body() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/echo",
        None,
        Some(json!({"message": "hello"})),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["echo"], "hello");
}

#[tokio::test]
async fn generic_post_route_dispatches_registered_action() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/greet",
        None,
        Some(json!({"name": "Registry"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["greeting"], "Hello, Registry!");
}

#[tokio::test]
async fn generic_post_route_rejects_unknown_fields() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/echo",
        None,
        Some(json!({"message": "hello", "extra": true})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["error"]
        .as_str()
        .unwrap_or_default()
        .contains("unknown parameter"));
}

#[tokio::test]
async fn removed_rest_envelope_is_not_found() {
    let app = server::router(loopback_state());
    let (status, _body) = request_json(
        app,
        Method::POST,
        "/v1/example",
        None,
        Some(json!({"action": "echo", "params": {"message": "hello"}})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[test]
fn rest_routes_match_action_registry_metadata() {
    for spec in rtemplate_service::action_specs()
        .iter()
        .filter(|spec| spec.transport.rest())
    {
        let method = spec
            .rest_method
            .unwrap_or_else(|| panic!("{} should declare a REST method", spec.name));
        let path = spec
            .rest_path
            .unwrap_or_else(|| panic!("{} should declare a REST path", spec.name));
        assert!(
            rest_routes().iter().any(|route| {
                route.action.as_deref() == Some(spec.name)
                    && route.method == method
                    && route.path == path
            }),
            "{} should be exposed as {method} {path}",
            spec.name
        );
    }

    for route in rest_routes().iter().filter(|route| route.action.is_some()) {
        let action = route.action.as_deref().unwrap();
        let spec = rtemplate_service::action_specs()
            .iter()
            .find(|spec| spec.name == action)
            .unwrap_or_else(|| panic!("REST route advertises unknown action `{action}`"));
        assert_eq!(spec.rest_method, Some(route.method.as_str()));
        assert_eq!(spec.rest_path, Some(route.path.as_str()));
    }
}

#[tokio::test]
async fn advertised_action_routes_are_mounted() {
    for spec in rtemplate_service::action_specs()
        .iter()
        .filter(|spec| spec.transport.rest())
    {
        let method = spec.rest_method.expect("REST action should have method");
        let path = spec.rest_path.expect("REST action should have path");
        let method = Method::from_bytes(method.as_bytes()).expect("method should parse");
        let params = rest_params_for(spec.name);
        let (status, body) =
            request_json(server::router(loopback_state()), method, path, None, params).await;
        assert_ne!(status, StatusCode::NOT_FOUND, "{spec:?} returned {body}");
        assert_ne!(
            status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{spec:?} returned {body}"
        );
    }
}

fn rest_params_for(action: &str) -> Option<Value> {
    match action {
        "greet" => Some(json!({"name": "Route"})),
        "echo" => Some(json!({"message": "route"})),
        _ => None,
    }
}

#[tokio::test]
async fn direct_rest_greet_accepts_empty_typed_body() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(app, Method::POST, "/v1/greet", None, Some(json!({}))).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["target"], "World");
}

#[tokio::test]
async fn direct_rest_validation_errors_are_bad_requests() {
    let app = server::router(loopback_state());
    for body in [
        json!({}),
        json!({"message": ""}),
        json!({"message": 42}),
        json!({"message": "hello", "extra": true}),
        json!({"message": "hello", "action": "echo"}),
    ] {
        let (status, response) =
            request_json(app.clone(), Method::POST, "/v1/echo", None, Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
        assert!(response.get("error").is_some(), "{response}");
    }
}

#[tokio::test]
async fn mcp_only_actions_are_not_available_as_generic_rest_posts() {
    let app = server::router(loopback_state());
    for action in ["elicit_name", "scaffold_intent"] {
        let (status, response) = request_json(
            app.clone(),
            Method::POST,
            &format!("/v1/{action}"),
            None,
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{response}");
    }
}

#[tokio::test]
async fn direct_rest_help_excludes_mcp_only_actions_from_rest_actions() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(app, Method::GET, "/v1/help", None, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["actions"], json!(["greet", "echo", "status", "help"]));
    assert_eq!(
        body["mcp_only_actions"],
        json!(["elicit_name", "scaffold_intent"])
    );
}

#[tokio::test]
async fn capabilities_advertises_direct_rest_routes() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(app, Method::GET, "/v1/capabilities", None, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["preferred_rest_style"], "direct_routes");
    assert!(body["supported_routes"]
        .as_array()
        .expect("supported_routes should be an array")
        .contains(&json!("POST /v1/echo")));
    assert!(!body["supported_routes"]
        .as_array()
        .expect("supported_routes should be an array")
        .contains(&json!("POST /v1/example")));
}

#[tokio::test]
async fn openapi_json_is_public_and_lists_direct_routes() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(app, Method::GET, "/openapi.json", None, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["openapi"], "3.1.0");
    assert!(body["paths"].get("/v1/echo").is_some());
    assert!(body["paths"].get("/v1/capabilities").is_some());
    assert!(body["paths"].get("/v1/example").is_none());
    assert_eq!(body["x-template"]["preferred_rest_style"], "direct_routes");
    assert!(
        body["components"]["schemas"]["StatusResponse"]["properties"]
            .get("api_url")
            .is_none(),
        "{body}"
    );
}

#[tokio::test]
async fn status_returns_only_local_redacted_metadata() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(app, Method::GET, "/status", None, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["server"], "rtemplate-mcp");
    assert_eq!(body["transport"], "http");
    assert!(body.get("version").is_some());
    assert!(body.get("api_url").is_none(), "{body}");
    assert!(body.get("api_key").is_none(), "{body}");
    assert!(body.get("upstream").is_none(), "{body}");
}

#[tokio::test]
async fn mounted_bearer_auth_protects_rest_endpoint() {
    let app = server::router(bearer_state("secret"));

    let (missing_status, _) =
        request_json(app.clone(), Method::GET, "/v1/status", None, None).await;
    assert_eq!(missing_status, StatusCode::UNAUTHORIZED);

    let (valid_status, valid_body) =
        request_json(app, Method::GET, "/v1/status", Some("secret"), None).await;
    assert_eq!(valid_status, StatusCode::OK);
    assert_eq!(valid_body["status"], "ok");
}

#[tokio::test]
async fn trusted_gateway_unscoped_bypasses_local_auth() {
    let mut state = loopback_state();
    state.auth_policy = AuthPolicy::TrustedGatewayUnscoped;
    let app = server::router(state);
    let (status, body) = request_json(app, Method::GET, "/v1/status", None, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn oversized_body_returns_413() {
    // The router mounts RequestBodyLimitLayer at 65_536 bytes (64 KiB).
    // A body one byte over the limit must be rejected with HTTP 413.
    let app = server::router(loopback_state());
    let oversized_body = vec![b'x'; 65_537];
    let request = Request::builder()
        .method(Method::POST)
        .uri("/v1/echo")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(oversized_body))
        .expect("request should build");

    let response = app.oneshot(request).await.expect("route should respond");

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
