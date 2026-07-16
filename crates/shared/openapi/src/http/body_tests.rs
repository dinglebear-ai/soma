use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn spec_body_cap_returns_spec_too_large() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("abcdef"))
        .mount(&server)
        .await;

    let response = super::client::build_loopback_test_client()
        .get(server.uri())
        .send()
        .await
        .unwrap();
    let err = super::body::collect_spec_capped(response, 3, "vendor")
        .await
        .unwrap_err();
    assert_eq!(err.kind(), "config_error");
}

#[tokio::test]
async fn response_body_cap_returns_scrubbed_upstream_error() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("secret-body"))
        .mount(&server)
        .await;

    let response = super::client::build_loopback_test_client()
        .get(server.uri())
        .send()
        .await
        .unwrap();
    let err = super::body::collect_response_capped(response, 3, "getUser")
        .await
        .unwrap_err();
    assert_eq!(err.kind(), "internal_error");
    assert!(!err.to_string().contains("secret-body"));
}
