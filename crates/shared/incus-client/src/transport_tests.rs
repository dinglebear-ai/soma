use super::*;

#[tokio::test]
async fn request_parses_a_sync_envelope() {
    let body = r#"{"type":"sync","status":"Success","status_code":200,"metadata":{"name":"c1"}}"#;
    let (socket_path, _dir) = crate::transport::unix::tests::spawn_fake_daemon(move |_req| {
        crate::transport::unix::tests::json_response("HTTP/1.1 200 OK", body)
    })
    .await;

    let client = Client::new(ClientConfig::unix_socket(socket_path));
    let envelope = client
        .request(Method::Get, "/1.0/instances/c1", &[], None, None)
        .await
        .expect("request should succeed");

    match envelope {
        IncusEnvelope::Sync { metadata, .. } => assert_eq!(metadata["name"], "c1"),
        other => panic!("expected Sync envelope, got {other:?}"),
    }
}

#[tokio::test]
async fn request_parses_an_async_envelope() {
    let body = r#"{"type":"async","status":"Operation created","status_code":100,"operation":"/1.0/operations/11111111-1111-1111-1111-111111111111","metadata":{"id":"11111111-1111-1111-1111-111111111111","class":"task","status":"Running","status_code":103,"resources":{},"may_cancel":false,"err":null}}"#;
    let (socket_path, _dir) = crate::transport::unix::tests::spawn_fake_daemon(move |_req| {
        crate::transport::unix::tests::json_response("HTTP/1.1 202 Accepted", body)
    })
    .await;

    let client = Client::new(ClientConfig::unix_socket(socket_path));
    let envelope = client
        .request(Method::Post, "/1.0/instances", &[], None, None)
        .await
        .expect("request should succeed");

    match envelope {
        IncusEnvelope::Async { metadata } => {
            assert_eq!(metadata["class"], "task");
        }
        other => panic!("expected Async envelope, got {other:?}"),
    }
}

#[tokio::test]
async fn request_rejects_an_async_envelope_missing_the_operation_field() {
    // IncusEnvelope::Async no longer stores the "operation" URL (nothing in
    // this crate reads it - operation_from_envelope derives the operation ID
    // from metadata.id instead), but the field's *presence* is still
    // validated for envelope-shape strictness against the documented Incus
    // response.
    let body = r#"{"type":"async","status":"Operation created","status_code":100,"metadata":{"id":"11111111-1111-1111-1111-111111111111","class":"task","status":"Running","status_code":103,"resources":{},"may_cancel":false,"err":null}}"#;
    let (socket_path, _dir) = crate::transport::unix::tests::spawn_fake_daemon(move |_req| {
        crate::transport::unix::tests::json_response("HTTP/1.1 202 Accepted", body)
    })
    .await;

    let client = Client::new(ClientConfig::unix_socket(socket_path));
    let err = client
        .request(Method::Post, "/1.0/instances", &[], None, None)
        .await
        .expect_err("an async envelope with no \"operation\" field must be rejected");

    assert!(matches!(err, crate::Error::InvalidResponse(_)));
}

#[tokio::test]
async fn request_maps_a_4xx_status_to_error_api() {
    let body = r#"{"type":"error","error":"not found","error_code":404}"#;
    let (socket_path, _dir) = crate::transport::unix::tests::spawn_fake_daemon(move |_req| {
        crate::transport::unix::tests::json_response("HTTP/1.1 404 Not Found", body)
    })
    .await;

    let client = Client::new(ClientConfig::unix_socket(socket_path));
    let err = client
        .request(Method::Get, "/1.0/instances/missing", &[], None, None)
        .await
        .expect_err("a 404 status must surface as an error");

    match err {
        crate::Error::Api {
            status_code,
            message,
        } => {
            assert_eq!(status_code, 404);
            assert_eq!(message, "not found");
        }
        other => panic!("expected Error::Api, got {other:?}"),
    }
}
