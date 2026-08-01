//! Route-level tests for REST dispatch, status, and mounted auth behavior.
#![cfg(feature = "mcp-http")]

use async_trait::async_trait;
use axum::{
    body::{Body, Bytes},
    http::{HeaderMap, Method, Request, StatusCode, header},
    response::IntoResponse,
    routing::post,
};
use serde_json::{Value, json};
use soma::{
    api::REST_ROUTES,
    server::{self, AuthPolicy},
    testing::{bearer_state, loopback_state, loopback_state_with_registry},
};
use soma_application::ProviderError;
use soma_application::provider_registry::{
    Provider, ProviderCall, ProviderOutput, ProviderRegistry,
};
use soma_domain::actions::ACTION_SPECS;
use soma_gateway::config::{GatewayConfig, ProtectedMcpRouteConfig, UpstreamConfig};
use soma_provider_core::{
    ProviderCatalog, ProviderIdentity, ProviderKind, ProviderManifest, ProviderTool, RestOverlay,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower::ServiceExt;

mod support;
use support::request_json;

#[tokio::test]
async fn protected_route_proxy_does_not_forward_inbound_trace_headers() {
    let seen_headers: Arc<Mutex<Vec<HeaderMap>>> = Arc::new(Mutex::new(Vec::new()));
    let backend = header_capturing_backend_server(seen_headers.clone()).await;
    let temp = tempfile::tempdir().unwrap();
    let state = soma::testing::oauth_state_with_gateway(
        temp.path(),
        protected_gateway_config(Some(backend), None),
    )
    .await;
    let token = protected_route_token(&state, "https://mcp.example.com/media", "soma:read");

    let response = server::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/media")
                .header(header::HOST, "mcp.example.com")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(
                    "traceparent",
                    "00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01",
                )
                .header("tracestate", "vendor=value")
                .header("baggage", "region=us-east-1")
                .body(Body::from(r#"{"jsonrpc":"2.0","id":1}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let seen = seen_headers.lock().await;
    assert_eq!(seen.len(), 1);
    for name in ["traceparent", "tracestate", "baggage"] {
        assert!(
            !seen[0].contains_key(name),
            "outbound proxied request must not carry {name}, got: {:?}",
            seen[0]
        );
    }
}

fn provider_tool(name: &str, description: &str, input_schema: Value) -> ProviderTool {
    ProviderTool {
        name: name.to_owned(),
        description: description.to_owned(),
        title: None,
        input_schema,
        output_schema: None,
        scope: Some("soma:read".to_owned()),
        destructive: false,
        requires_admin: false,
        cost: Some("cheap".to_owned()),
        env: Vec::new(),
        limits: None,
        mcp: None,
        rest: None,
        cli: None,
        palette: None,
        ui: None,
        examples: Vec::new(),
        meta: json!({}),
    }
}

fn protected_gateway_config(
    upstream_url: Option<String>,
    bearer_token_env: Option<&str>,
) -> GatewayConfig {
    GatewayConfig {
        upstream: upstream_url
            .map(|url| UpstreamConfig {
                name: "backend".to_owned(),
                url: Some(url),
                bearer_token_env: bearer_token_env.map(ToOwned::to_owned),
                ..UpstreamConfig::default()
            })
            .into_iter()
            .collect(),
        protected_mcp_routes: vec![ProtectedMcpRouteConfig {
            name: "media".to_owned(),
            public_host: "mcp.example.com".to_owned(),
            public_path: "/media".to_owned(),
            upstream: Some("backend".to_owned()),
            scopes: vec!["soma:read".to_owned()],
            ..ProtectedMcpRouteConfig::default()
        }],
        ..GatewayConfig::default()
    }
}

fn protected_route_token(state: &server::AppState, audience: &str, scope: &str) -> String {
    let AuthPolicy::Mounted {
        auth_state: Some(auth_state),
    } = &state.auth_policy
    else {
        panic!("test state must use OAuth auth policy");
    };
    auth_state
        .signing_keys
        .issue_access_token(&soma_auth::jwt::AccessClaims {
            iss: "https://example.example.com".to_owned(),
            sub: "google-user".to_owned(),
            aud: audience.to_owned(),
            exp: 4_102_444_800,
            iat: 1_700_000_000,
            jti: "protected-route-test".to_owned(),
            scope: scope.to_owned(),
            azp: "client".to_owned(),
        })
        .unwrap()
}

async fn header_capturing_backend_server(seen_headers: Arc<Mutex<Vec<HeaderMap>>>) -> String {
    let app = axum::Router::new().route(
        "/mcp",
        post(move |headers: HeaderMap, _body: Bytes| {
            let seen_headers = seen_headers.clone();
            async move {
                seen_headers.lock().await.push(headers);
                (StatusCode::OK, "proxied").into_response()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/mcp")
}

#[derive(Clone)]
struct RestDynamicProvider;

#[async_trait]
impl Provider for RestDynamicProvider {
    fn catalog(&self) -> ProviderCatalog {
        let weather_schema = json!({
            "type": "object",
            "required": ["city"],
            "additionalProperties": false,
            "properties": {"city": {"type": "string"}}
        });
        let empty_schema = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {}
        });
        ProviderManifest {
            schema_version: 1,
            provider: ProviderIdentity {
                name: "dynamic-rest".to_owned(),
                kind: ProviderKind::StaticRust,
                title: None,
                description: None,
                homepage: None,
                source: None,
                version: None,
                enabled: Some(true),
            },
            tools: vec![
                ProviderTool {
                    rest: Some(RestOverlay {
                        enabled: true,
                        method: Some("POST".to_owned()),
                        path: Some("/v1/weather".to_owned()),
                        tags: vec!["dynamic".to_owned()],
                        summary: None,
                        description: None,
                        deprecated: false,
                        path_params: json!({}),
                        query_params: json!({}),
                        request_body_schema: None,
                    }),
                    ..provider_tool("weather", "Fetch weather", weather_schema)
                },
                provider_tool(
                    "runtime_check",
                    "Check the provider runtime",
                    empty_schema.clone(),
                ),
                ProviderTool {
                    rest: Some(RestOverlay {
                        enabled: false,
                        method: None,
                        path: None,
                        tags: Vec::new(),
                        summary: None,
                        description: None,
                        deprecated: false,
                        path_params: json!({}),
                        query_params: json!({}),
                        request_body_schema: None,
                    }),
                    ..provider_tool("hidden", "Hidden REST tool", empty_schema)
                },
            ],
            prompts: Vec::new(),
            resources: Vec::new(),
            tasks: Vec::new(),
            elicitation: Vec::new(),
            env: Vec::new(),
            capabilities: Default::default(),
            docs: None,
            plugin: None,
            ui: None,
            meta: json!({}),
        }
    }

    async fn call(&self, call: ProviderCall) -> Result<ProviderOutput, ProviderError> {
        Ok(ProviderOutput::json(json!({
            "provider": call.provider,
            "action": call.action,
            "city": call.params["city"],
        })))
    }
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
    assert_eq!(body["output"]["echo"], "hello");
}

#[tokio::test]
async fn dynamic_provider_rest_route_dispatches_from_registry_snapshot() {
    let state = loopback_state_with_registry(
        ProviderRegistry::new(vec![Arc::new(RestDynamicProvider)]).expect("dynamic registry"),
    );
    let app = server::router(state);
    let (status, body) = request_json(
        app,
        Method::POST,
        "/v1/weather",
        None,
        Some(json!({"city": "Paris"})),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["output"]["provider"], "dynamic-rest");
    assert_eq!(body["output"]["action"], "weather");
    assert_eq!(body["output"]["city"], "Paris");
}

#[tokio::test]
async fn generic_provider_tool_route_dispatches_tools_without_custom_rest_overlay() {
    let state = loopback_state_with_registry(
        ProviderRegistry::new(vec![Arc::new(RestDynamicProvider)]).expect("dynamic registry"),
    );
    let app = server::router(state);
    let (status, body) =
        request_json(app, Method::POST, "/v1/tools/runtime_check", None, None).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["output"]["provider"], "dynamic-rest");
    assert_eq!(body["output"]["action"], "runtime_check");
}

#[tokio::test]
async fn generic_provider_tool_route_respects_explicit_rest_disable() {
    let state = loopback_state_with_registry(
        ProviderRegistry::new(vec![Arc::new(RestDynamicProvider)]).expect("dynamic registry"),
    );
    let app = server::router(state);
    let (status, body) =
        request_json(app, Method::POST, "/v1/tools/hidden", None, Some(json!({}))).await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["code"], "surface_not_exposed");
}

#[tokio::test]
async fn providers_endpoint_lists_live_provider_rest_tools() {
    let state = loopback_state_with_registry(
        ProviderRegistry::new(vec![Arc::new(RestDynamicProvider)]).expect("dynamic registry"),
    );
    let app = server::router(state);
    let (status, body) = request_json(app, Method::GET, "/v1/providers", None, None).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["providers"][0]["name"], "dynamic-rest");
    assert_eq!(body["providers"][0]["tools"][0]["name"], "weather");
    assert_eq!(
        body["providers"][0]["tools"][0]["input_schema"]["properties"]["city"]["type"],
        "string"
    );
    assert_eq!(
        body["providers"][0]["tools"][0]["rest"]["path"],
        "/v1/weather"
    );
    assert_eq!(body["providers"][0]["tools"][1]["name"], "runtime_check");
    assert_eq!(
        body["providers"][0]["tools"][1]["generic_rest"]["path"],
        "/v1/tools/runtime_check"
    );
    assert_eq!(body["providers"][0]["tools"][1]["surfaces"]["rest"], true);
    assert_eq!(body["providers"][0]["tools"][2]["name"], "hidden");
    assert_eq!(
        body["providers"][0]["tools"][2]["generic_rest"],
        Value::Null
    );
    assert_eq!(body["providers"][0]["tools"][2]["surfaces"]["rest"], false);
}

#[test]
fn rest_routes_match_action_registry_metadata() {
    for spec in ACTION_SPECS.iter().filter(|spec| spec.transport.rest()) {
        let method = spec
            .rest_method
            .unwrap_or_else(|| panic!("{} should declare a REST method", spec.name));
        let path = spec
            .rest_path
            .unwrap_or_else(|| panic!("{} should declare a REST path", spec.name));
        assert!(
            REST_ROUTES.iter().any(|route| {
                route.action == Some(spec.name) && route.method == method && route.path == path
            }),
            "{} should be exposed as {method} {path}",
            spec.name
        );
    }

    for route in REST_ROUTES.iter().filter(|route| route.action.is_some()) {
        let action = route.action.unwrap();
        let spec = ACTION_SPECS
            .iter()
            .find(|spec| spec.name == action)
            .unwrap_or_else(|| panic!("REST route advertises unknown action `{action}`"));
        assert_eq!(spec.rest_method, Some(route.method));
        assert_eq!(spec.rest_path, Some(route.path));
    }
}

#[tokio::test]
async fn direct_rest_greet_accepts_empty_typed_body() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(app, Method::POST, "/v1/greet", None, Some(json!({}))).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["output"]["target"], "World");
}

#[tokio::test]
async fn direct_rest_validation_errors_are_bad_requests() {
    let app = server::router(loopback_state());
    for body in [
        json!({}),
        json!({"message": ""}),
        json!({"message": 42}),
        json!({"message": "hello", "extra": true}),
    ] {
        let (status, response) =
            request_json(app.clone(), Method::POST, "/v1/echo", None, Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
        assert!(response.get("error").is_some(), "{response}");
    }
}

#[tokio::test]
async fn direct_rest_help_excludes_mcp_only_actions_from_rest_actions() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(app, Method::GET, "/v1/help", None, None).await;
    let expected_rest_actions = ACTION_SPECS
        .iter()
        .filter(|spec| spec.rest_method.is_some())
        .map(|spec| spec.name)
        .collect::<Vec<_>>();
    let expected_mcp_only_actions = ACTION_SPECS
        .iter()
        .filter(|spec| spec.rest_method.is_none())
        .map(|spec| spec.name)
        .collect::<Vec<_>>();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["output"]["actions"], json!(expected_rest_actions));
    assert_eq!(
        body["output"]["mcp_only_actions"],
        json!(expected_mcp_only_actions)
    );
}

#[tokio::test]
async fn capabilities_advertises_direct_rest_routes() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(app, Method::GET, "/v1/capabilities", None, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["preferred_rest_style"], "direct_routes");
    assert!(
        body["supported_routes"]
            .as_array()
            .expect("supported_routes should be an array")
            .contains(&json!("POST /v1/echo"))
    );
    assert!(
        body["supported_routes"]
            .as_array()
            .expect("supported_routes should be an array")
            .contains(&json!("POST /v1/tools/{action}"))
    );
    assert!(
        body["supported_routes"]
            .as_array()
            .expect("supported_routes should be an array")
            .contains(&json!("POST /v1/gateway/{action}"))
    );
    assert!(
        !body["supported_routes"]
            .as_array()
            .expect("supported_routes should be an array")
            .contains(&json!("POST /v1/soma"))
    );
}

#[tokio::test]
async fn openapi_json_is_public_and_lists_direct_routes() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(app, Method::GET, "/openapi.json", None, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["openapi"], "3.1.0");
    assert!(body["paths"].get("/v1/echo").is_some());
    assert!(body["paths"].get("/v1/capabilities").is_some());
    assert!(body["paths"].get("/v1/gateway/{action}").is_some());
    assert!(body["paths"].get("/v1/soma").is_none());
    assert_eq!(body["x-soma"]["preferred_rest_style"], "direct_routes");
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
    assert_eq!(body["server"], "soma");
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
    assert_eq!(valid_body["output"]["status"], "ok");
}

#[tokio::test]
async fn palette_catalog_is_reachable_through_the_composed_router() {
    let app = server::router(loopback_state());
    let (status, body) = request_json(app, Method::GET, "/v1/palette/catalog", None, None).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.get("entries").is_some(), "{body}");
}

#[tokio::test]
async fn mounted_bearer_auth_protects_palette_routes() {
    let app = server::router(bearer_state("secret"));

    let (missing_status, _) =
        request_json(app.clone(), Method::GET, "/v1/palette/catalog", None, None).await;
    assert_eq!(missing_status, StatusCode::UNAUTHORIZED);

    let (valid_status, _) = request_json(
        app,
        Method::GET,
        "/v1/palette/catalog",
        Some("secret"),
        None,
    )
    .await;
    assert_eq!(valid_status, StatusCode::OK);
}

#[tokio::test]
async fn trusted_gateway_unscoped_bypasses_local_auth() {
    let mut state = loopback_state();
    state.auth_policy = AuthPolicy::TrustedGatewayUnscoped;
    let app = server::router(state);
    let (status, body) = request_json(app, Method::GET, "/v1/status", None, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["output"]["status"], "ok");
}

#[tokio::test]
async fn public_health_is_unaffected_by_gateway_auth() {
    let app = server::router(bearer_state("secret"));
    let (status, body) = request_json(app, Method::GET, "/health", None, None).await;

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
