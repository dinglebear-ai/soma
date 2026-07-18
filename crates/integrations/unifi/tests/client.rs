//! End-to-end HTTP tests against a mocked controller.
//!
//! This is the pattern to copy for testing an integration crate's HTTP layer
//! without a real upstream service: spin up a [`wiremock::MockServer`], point
//! the client at it, and assert on the mapped [`UnifiError`] variant (not a
//! stringified message) for failure cases.

use serde_json::json;
use unifi::{UnifiClient, UnifiConfig, UnifiError};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn config(url: String) -> UnifiConfig {
    UnifiConfig {
        url,
        api_key: "test-key".to_string(),
        site: "default".to_string(),
        skip_tls_verify: true,
        legacy: false,
        ..UnifiConfig::default()
    }
}

#[tokio::test]
async fn clients_returns_the_parsed_body_on_success() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/proxy/network/api/s/default/stat/sta"))
        .and(header("X-API-KEY", "test-key"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "data": [{"mac": "aa:bb"}] })),
        )
        .mount(&server)
        .await;

    let client = UnifiClient::new(&config(server.uri())).unwrap();

    let result = client.clients().await.unwrap();

    assert_eq!(result, json!({ "data": [{"mac": "aa:bb"}] }));
}

#[tokio::test]
async fn an_expired_api_key_maps_to_unauthorized() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/proxy/network/api/s/default/stat/sta"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let client = UnifiClient::new(&config(server.uri())).unwrap();

    let err = client.clients().await.unwrap_err();

    match err {
        UnifiError::Unauthorized(url) => {
            assert!(
                url.ends_with("/proxy/network/api/s/default/stat/sta"),
                "unexpected url: {url}"
            );
        }
        other => panic!("expected Unauthorized, got {other:?}"),
    }
}

#[tokio::test]
async fn a_missing_endpoint_maps_to_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/proxy/network/api/s/default/stat/sta"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client = UnifiClient::new(&config(server.uri())).unwrap();

    let err = client.clients().await.unwrap_err();

    match err {
        UnifiError::NotFound { method, url } => {
            assert_eq!(method, "GET");
            assert!(
                url.ends_with("/proxy/network/api/s/default/stat/sta"),
                "unexpected url: {url}"
            );
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn an_empty_body_on_a_get_maps_to_empty_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/proxy/network/api/s/default/stat/sta"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let client = UnifiClient::new(&config(server.uri())).unwrap();

    let err = client.clients().await.unwrap_err();

    match err {
        UnifiError::EmptyBody { method, url } => {
            assert_eq!(method, "GET");
            assert!(
                url.ends_with("/proxy/network/api/s/default/stat/sta"),
                "unexpected url: {url}"
            );
        }
        other => panic!("expected EmptyBody, got {other:?}"),
    }
}

#[tokio::test]
async fn an_unexpected_status_carries_the_response_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/proxy/network/api/s/default/stat/sta"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({ "error": "boom" })))
        .mount(&server)
        .await;

    let client = UnifiClient::new(&config(server.uri())).unwrap();

    let err = client.clients().await.unwrap_err();

    match err {
        UnifiError::UnexpectedStatus { status, body, .. } => {
            assert_eq!(status, 500);
            assert_eq!(*body, json!({ "error": "boom" }));
        }
        other => panic!("expected UnexpectedStatus, got {other:?}"),
    }
}

#[tokio::test]
async fn an_unexpected_status_with_a_non_json_body_still_reports_the_status() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/proxy/network/api/s/default/stat/sta"))
        .respond_with(ResponseTemplate::new(502).set_body_string("<html>Bad Gateway</html>"))
        .mount(&server)
        .await;

    let client = UnifiClient::new(&config(server.uri())).unwrap();

    let err = client.clients().await.unwrap_err();

    match err {
        UnifiError::UnexpectedStatus { status, body, .. } => {
            assert_eq!(status, 502);
            assert_eq!(*body, json!("<html>Bad Gateway</html>"));
        }
        other => panic!("expected UnexpectedStatus, got {other:?}"),
    }
}

#[tokio::test]
async fn a_rate_limited_response_carries_the_parsed_retry_after() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/proxy/network/api/s/default/stat/sta"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "30"))
        .mount(&server)
        .await;

    let client = UnifiClient::new(&config(server.uri())).unwrap();

    let err = client.clients().await.unwrap_err();

    match err {
        UnifiError::RateLimited {
            method,
            url,
            retry_after,
        } => {
            assert_eq!(method, "GET");
            assert!(
                url.ends_with("/proxy/network/api/s/default/stat/sta"),
                "unexpected url: {url}"
            );
            assert_eq!(retry_after, Some(std::time::Duration::from_secs(30)));
        }
        other => panic!("expected RateLimited, got {other:?}"),
    }
}

#[tokio::test]
async fn a_rate_limited_response_without_retry_after_leaves_it_none() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/proxy/network/api/s/default/stat/sta"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;

    let client = UnifiClient::new(&config(server.uri())).unwrap();

    let err = client.clients().await.unwrap_err();

    match err {
        UnifiError::RateLimited { retry_after, .. } => assert_eq!(retry_after, None),
        other => panic!("expected RateLimited, got {other:?}"),
    }
}

#[test]
fn new_rejects_a_missing_url() {
    let err = UnifiClient::new(&UnifiConfig {
        url: String::new(),
        api_key: "key".to_string(),
        ..UnifiConfig::default()
    })
    .unwrap_err();

    assert!(matches!(err, UnifiError::MissingUrl));
}

#[test]
fn new_rejects_a_missing_api_key() {
    let err = UnifiClient::new(&UnifiConfig {
        url: "https://unifi.local".to_string(),
        api_key: String::new(),
        ..UnifiConfig::default()
    })
    .unwrap_err();

    assert!(matches!(err, UnifiError::MissingApiKey));
}
