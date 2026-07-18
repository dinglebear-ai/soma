//! End-to-end HTTP tests against a mocked server.
//!
//! This is the pattern to copy for testing an integration crate's HTTP layer
//! without a real upstream service: spin up a [`wiremock::MockServer`], point
//! the client at it, and assert on the mapped [`GotifyError`] variant (not a
//! stringified message) for failure cases.

use gotify::{GotifyClient, GotifyConfig, GotifyError};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn config(url: String) -> GotifyConfig {
    GotifyConfig {
        url,
        client_token: "test-client-token".to_string(),
        app_token: "test-app-token".to_string(),
        ..GotifyConfig::default()
    }
}

#[tokio::test]
async fn health_returns_the_parsed_body_on_success() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "health": "green" })))
        .mount(&server)
        .await;

    let client = GotifyClient::new(&config(server.uri())).unwrap();

    let result = client.health().await.unwrap();

    assert_eq!(result, json!({ "health": "green" }));
}

#[tokio::test]
async fn messages_sends_the_client_token_not_the_app_token() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/message"))
        .and(header("X-Gotify-Key", "test-client-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "messages": [] })))
        .mount(&server)
        .await;

    let client = GotifyClient::new(&config(server.uri())).unwrap();

    let result = client.messages(None, None, None).await.unwrap();

    assert_eq!(result, json!({ "messages": [] }));
}

#[tokio::test]
async fn send_message_sends_the_app_token_not_the_client_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/message"))
        .and(header("X-Gotify-Key", "test-app-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": 1 })))
        .mount(&server)
        .await;

    let client = GotifyClient::new(&config(server.uri())).unwrap();

    let result = client
        .send_message("hello", None, None, None)
        .await
        .unwrap();

    assert_eq!(result, json!({ "id": 1 }));
}

#[tokio::test]
async fn messages_without_a_client_token_configured_fails_before_any_request_is_sent() {
    // No mocks registered — wiremock fails loudly (panics) if a request goes
    // out unexpectedly, so this also proves the client fails fast on a
    // missing token instead of sending an unauthenticated request.
    let server = MockServer::start().await;
    let client = GotifyClient::new(&GotifyConfig {
        url: server.uri(),
        ..GotifyConfig::default()
    })
    .unwrap();

    let err = client.messages(None, None, None).await.unwrap_err();

    assert!(matches!(err, GotifyError::MissingClientToken));
}

#[tokio::test]
async fn send_message_without_an_app_token_configured_fails_before_any_request_is_sent() {
    let server = MockServer::start().await;
    let client = GotifyClient::new(&GotifyConfig {
        url: server.uri(),
        client_token: "test-client-token".to_string(),
        ..GotifyConfig::default()
    })
    .unwrap();

    let err = client
        .send_message("hello", None, None, None)
        .await
        .unwrap_err();

    assert!(matches!(err, GotifyError::MissingAppToken));
}

#[tokio::test]
async fn an_expired_token_maps_to_unauthorized() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/message"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let client = GotifyClient::new(&config(server.uri())).unwrap();

    let err = client.messages(None, None, None).await.unwrap_err();

    match err {
        GotifyError::Unauthorized(url) => {
            assert!(url.ends_with("/message"), "unexpected url: {url}");
        }
        other => panic!("expected Unauthorized, got {other:?}"),
    }
}

#[tokio::test]
async fn a_missing_endpoint_maps_to_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/message"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client = GotifyClient::new(&config(server.uri())).unwrap();

    let err = client.messages(None, None, None).await.unwrap_err();

    match err {
        GotifyError::NotFound { method, url } => {
            assert_eq!(method, "GET");
            assert!(url.ends_with("/message"), "unexpected url: {url}");
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn a_rate_limited_response_carries_the_parsed_retry_after() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/message"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "30"))
        .mount(&server)
        .await;

    let client = GotifyClient::new(&config(server.uri())).unwrap();

    let err = client.messages(None, None, None).await.unwrap_err();

    match err {
        GotifyError::RateLimited {
            method,
            url,
            retry_after,
        } => {
            assert_eq!(method, "GET");
            assert!(url.ends_with("/message"), "unexpected url: {url}");
            assert_eq!(retry_after, Some(std::time::Duration::from_secs(30)));
        }
        other => panic!("expected RateLimited, got {other:?}"),
    }
}

#[tokio::test]
async fn a_rate_limited_response_without_retry_after_leaves_it_none() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/message"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;

    let client = GotifyClient::new(&config(server.uri())).unwrap();

    let err = client.messages(None, None, None).await.unwrap_err();

    match err {
        GotifyError::RateLimited { retry_after, .. } => assert_eq!(retry_after, None),
        other => panic!("expected RateLimited, got {other:?}"),
    }
}

#[tokio::test]
async fn an_unexpected_status_carries_the_response_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/message"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({ "error": "boom" })))
        .mount(&server)
        .await;

    let client = GotifyClient::new(&config(server.uri())).unwrap();

    let err = client.messages(None, None, None).await.unwrap_err();

    match err {
        GotifyError::UnexpectedStatus { status, body, .. } => {
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
        .and(path("/message"))
        .respond_with(ResponseTemplate::new(502).set_body_string("<html>Bad Gateway</html>"))
        .mount(&server)
        .await;

    let client = GotifyClient::new(&config(server.uri())).unwrap();

    let err = client.messages(None, None, None).await.unwrap_err();

    match err {
        GotifyError::UnexpectedStatus { status, body, .. } => {
            assert_eq!(status, 502);
            assert_eq!(*body, json!("<html>Bad Gateway</html>"));
        }
        other => panic!("expected UnexpectedStatus, got {other:?}"),
    }
}

#[tokio::test]
async fn delete_message_handles_a_204_no_content_response() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/message/42"))
        .and(header("X-Gotify-Key", "test-client-token"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = GotifyClient::new(&config(server.uri())).unwrap();

    let result = client.delete_message(42).await.unwrap();

    assert_eq!(result, json!({ "status": "ok" }));
}

#[test]
fn new_rejects_a_missing_url() {
    let err = GotifyClient::new(&GotifyConfig::default()).unwrap_err();

    assert!(matches!(err, GotifyError::MissingUrl));
}
