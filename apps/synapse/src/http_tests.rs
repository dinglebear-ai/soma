use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;

use super::*;
use crate::SynapseConfig;

#[tokio::test]
async fn public_health_and_canonical_read_routes_work() {
    let runtime = Arc::new(StandaloneRuntime::from_config(SynapseConfig::default()).unwrap());
    let app = router(runtime);
    let health = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);

    let request = Request::builder()
        .method("POST")
        .uri("/v1/operations/product.help/execute")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"parameters":{}}"#))
        .unwrap();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn mutation_without_confirmation_returns_the_plan() {
    let runtime = Arc::new(StandaloneRuntime::from_config(SynapseConfig::default()).unwrap());
    let request = Request::builder()
        .method("POST")
        .uri("/v1/operations/container.start/execute")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"parameters":{"host":"local","container_id":"missing"}}"#,
        ))
        .unwrap();
    let response = router(runtime).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::PRECONDITION_REQUIRED);
}

#[tokio::test]
async fn activity_openapi_and_bearer_policy_are_product_owned() {
    let mut config = SynapseConfig::default();
    config.server.api_token = Some("secret-token".into());
    let runtime = Arc::new(StandaloneRuntime::from_config(config).unwrap());
    let app = router(Arc::clone(&runtime));

    let denied = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/operations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

    let allowed = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .header("authorization", "Bearer secret-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);

    runtime
        .execute(
            "product.help",
            &serde_json::json!({}),
            &ExecuteOptions {
                actor: Some("http-test".into()),
                ..Default::default()
            },
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(runtime.activity().len(), 1);
}
