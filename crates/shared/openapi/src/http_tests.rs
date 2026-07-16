use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::registry::OperationHandle;

fn handle(base: &str) -> OperationHandle {
    OperationHandle {
        operation_id: "getUser".into(),
        method: reqwest::Method::GET,
        path_template: "/users/{id}".into(),
        base_url: base.parse().unwrap(),
        credential: None,
    }
}

#[tokio::test]
async fn dispatch_disables_redirects() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/users/7"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/secret"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "leak": true })))
        .mount(&server)
        .await;

    let err = crate::http::execute_operation_no_ssrf(
        &crate::http::client::build_loopback_test_client(),
        &handle(&server.uri()),
        json!({ "id": "7" }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind(), "internal_error");
}

#[tokio::test]
async fn dispatch_rejects_oversized_response_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/users/7"))
        .respond_with(ResponseTemplate::new(200).set_body_string("abcdef"))
        .mount(&server)
        .await;

    let response = crate::http::client::build_loopback_test_client()
        .get(format!("{}/users/7", server.uri()))
        .send()
        .await
        .unwrap();
    let err = crate::http::body::collect_response_capped(response, 3, "getUser")
        .await
        .unwrap_err();
    assert_eq!(err.kind(), "internal_error");
}
