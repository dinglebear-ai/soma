use crate::config::ClientConfig;
use crate::transport::{
    unix::tests::{json_response, spawn_fake_daemon},
    Client, WithEtag,
};

fn image_json(fingerprint: &str) -> String {
    format!(
        r#"{{"type":"sync","status":"Success","status_code":200,"metadata":{{"fingerprint":"{fingerprint}","public":false,"filename":"debian-12.tar.xz","size":123456,"architecture":"x86_64","created_at":"2026-01-01T00:00:00Z","uploaded_at":"2026-01-01T00:00:00Z","properties":{{}}}}}}"#
    )
}

fn operation_json(id: &str) -> String {
    format!(
        r#"{{"type":"async","status":"Operation created","status_code":100,"operation":"/1.0/operations/{id}","metadata":{{"id":"{id}","class":"task","status":"Running","status_code":103,"resources":{{}},"may_cancel":true,"err":null}}}}"#
    )
}

#[tokio::test]
async fn get_image_deserializes_the_documented_shape_and_returns_etag() {
    let body = image_json("abc123");
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"img-etag\"\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        response.into_bytes()
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let WithEtag { value: image, etag } = client
        .get_image("abc123")
        .await
        .expect("get_image should succeed");

    assert_eq!(image.fingerprint, "abc123");
    assert_eq!(image.filename, "debian-12.tar.xz");
    assert_eq!(etag.as_deref(), Some("\"img-etag\""));
}

#[tokio::test]
async fn list_images_requires_explicit_recursion() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let body = r#"{"type":"sync","status":"Success","status_code":200,"metadata":[]}"#;
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        *seen.lock().unwrap() = String::from_utf8_lossy(&req).into_owned();
        json_response("HTTP/1.1 200 OK", body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    client
        .list_images(false)
        .await
        .expect("list_images should succeed");
    assert!(seen_request.lock().unwrap().contains("recursion=false"));
}

#[tokio::test]
async fn create_image_returns_an_operation() {
    let id = uuid::Uuid::new_v4().to_string();
    let body = operation_json(&id);
    let (socket_path, _dir) =
        spawn_fake_daemon(move |_req| json_response("HTTP/1.1 202 Accepted", &body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let op = client
        .create_image(&serde_json::json!({"source": {"type": "url", "url": "https://example.com/image.tar.xz"}}))
        .await
        .expect("create_image should return an Operation - image creation is documented as async");
    assert_eq!(op.id.to_string(), id);
}

#[tokio::test]
async fn delete_image_returns_an_operation() {
    let id = uuid::Uuid::new_v4().to_string();
    let body = operation_json(&id);
    let (socket_path, _dir) =
        spawn_fake_daemon(move |_req| json_response("HTTP/1.1 202 Accepted", &body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let op = client
        .delete_image("abc123")
        .await
        .expect("delete_image should return an Operation");
    assert_eq!(op.id.to_string(), id);
}

#[tokio::test]
async fn update_image_returns_an_operation() {
    let id = uuid::Uuid::new_v4().to_string();
    let body = operation_json(&id);
    let (socket_path, _dir) =
        spawn_fake_daemon(move |_req| json_response("HTTP/1.1 202 Accepted", &body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let op = client
        .update_image("abc123", &serde_json::json!({"public": true}), None)
        .await
        .expect("update_image should return an Operation, per the crate-wide convention");
    assert_eq!(op.id.to_string(), id);
}

#[tokio::test]
async fn update_image_sends_if_match_header_when_etag_is_provided() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let id = uuid::Uuid::new_v4().to_string();
    let body = operation_json(&id);
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        *seen.lock().unwrap() = String::from_utf8_lossy(&req).into_owned();
        json_response("HTTP/1.1 202 Accepted", &body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    client
        .update_image(
            "abc123",
            &serde_json::json!({"public": true}),
            Some("\"img-etag\""),
        )
        .await
        .expect("update_image should succeed");

    assert!(seen_request
        .lock()
        .unwrap()
        .contains("If-Match: \"img-etag\""));
}

#[tokio::test]
async fn update_image_guarded_derives_fingerprint_and_etag_from_a_real_fetch() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let image_body = image_json("abc123");
    let op_id = uuid::Uuid::new_v4().to_string();
    let op_body = operation_json(&op_id);
    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = call_count.clone();
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if n == 0 {
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"real-img-etag\"\r\nContent-Length: {}\r\n\r\n{image_body}",
                image_body.len()
            )
            .into_bytes()
        } else {
            *seen.lock().unwrap() = String::from_utf8_lossy(&req).into_owned();
            json_response("HTTP/1.1 202 Accepted", &op_body)
        }
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let fetched = client
        .get_image("abc123")
        .await
        .expect("get_image should succeed");

    client
        .update_image_guarded(&fetched, &serde_json::json!({"public": true}))
        .await
        .expect("update_image_guarded should succeed");

    let request_text = seen_request.lock().unwrap().clone();
    assert!(request_text.contains("PUT /1.0/images/abc123"));
    assert!(request_text.contains("If-Match: \"real-img-etag\""));
}

#[tokio::test]
async fn update_image_maps_412_to_precondition_failed() {
    let body = r#"{"type":"error","error":"stale etag","error_code":412}"#;
    let (socket_path, _dir) =
        spawn_fake_daemon(move |_req| json_response("HTTP/1.1 412 Precondition Failed", body))
            .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let err = client
        .update_image("abc123", &serde_json::json!({}), Some("\"stale\""))
        .await
        .expect_err("412 must map to a distinguishable error");

    assert!(matches!(
        err,
        crate::Error::PreconditionFailed { resource } if resource == "abc123"
    ));
}
