use crate::config::ClientConfig;
use crate::transport::{
    unix::tests::{json_response, spawn_fake_daemon},
    Client, WithEtag,
};

fn network_json(name: &str) -> String {
    format!(
        r#"{{"type":"sync","status":"Success","status_code":200,"metadata":{{"name":"{name}","type":"bridge","managed":true,"status":"Created","config":{{}}}}}}"#
    )
}

/// The real Incus daemon's `SyncResponseLocation(true, nil, ...)` shape for
/// create endpoints - `metadata` is JSON `null`, not an operation.
fn sync_created_json() -> &'static str {
    r#"{"type":"sync","status":"Success","status_code":200,"metadata":null}"#
}

/// The real Incus daemon's `EmptySyncResponse` shape for update/delete
/// endpoints - `metadata` is an empty object, not an operation.
fn empty_sync_json() -> &'static str {
    r#"{"type":"sync","status":"Success","status_code":200,"metadata":{}}"#
}

#[tokio::test]
async fn get_network_deserializes_the_documented_shape_and_returns_etag() {
    let body = network_json("incusbr0");
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"net-etag\"\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        response.into_bytes()
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let WithEtag {
        value: network,
        etag,
    } = client
        .get_network("incusbr0")
        .await
        .expect("get_network should succeed");

    assert_eq!(network.name, "incusbr0");
    assert_eq!(network.network_type, "bridge");
    assert_eq!(etag.as_deref(), Some("\"net-etag\""));
}

#[tokio::test]
async fn list_networks_requires_explicit_recursion() {
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
        .list_networks(true)
        .await
        .expect("list_networks should succeed");
    assert!(seen_request.lock().unwrap().contains("recursion=true"));
}

#[tokio::test]
async fn create_and_delete_network_are_synchronous() {
    // Real Incus (cmd/incusd/networks.go: networksPost, networkDelete)
    // always returns a sync response for these two endpoints - never an
    // operation - so this crate's create_network/delete_network return
    // Result<()>, not Result<Operation>.
    let (socket_path, _dir) =
        spawn_fake_daemon(move |_req| json_response("HTTP/1.1 200 OK", sync_created_json())).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    client
        .create_network(&serde_json::json!({"name": "br1", "type": "bridge"}))
        .await
        .expect("create_network should succeed synchronously");

    client
        .delete_network("br1")
        .await
        .expect("delete_network should succeed synchronously");
}

#[tokio::test]
async fn update_network_sends_if_match_header_when_etag_is_provided() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        *seen.lock().unwrap() = String::from_utf8_lossy(&req).into_owned();
        json_response("HTTP/1.1 200 OK", empty_sync_json())
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    client
        .update_network(
            "br1",
            &serde_json::json!({"config": {}}),
            Some("\"net-etag\""),
        )
        .await
        .expect("update_network should succeed");

    assert!(seen_request
        .lock()
        .unwrap()
        .contains("If-Match: \"net-etag\""));
}

#[tokio::test]
async fn update_network_guarded_derives_name_and_etag_from_a_real_fetch() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let network_body = network_json("br1");
    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = call_count.clone();
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if n == 0 {
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"real-net-etag\"\r\nContent-Length: {}\r\n\r\n{network_body}",
                network_body.len()
            )
            .into_bytes()
        } else {
            *seen.lock().unwrap() = String::from_utf8_lossy(&req).into_owned();
            json_response("HTTP/1.1 200 OK", empty_sync_json())
        }
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let fetched = client
        .get_network("br1")
        .await
        .expect("get_network should succeed");

    client
        .update_network_guarded(&fetched, &serde_json::json!({"config": {}}))
        .await
        .expect("update_network_guarded should succeed");

    let request_text = seen_request.lock().unwrap().clone();
    assert!(request_text.contains("PUT /1.0/networks/br1"));
    assert!(request_text.contains("If-Match: \"real-net-etag\""));
}

#[tokio::test]
async fn update_network_maps_412_to_precondition_failed() {
    let body = r#"{"type":"error","error":"stale etag","error_code":412}"#;
    let (socket_path, _dir) =
        spawn_fake_daemon(move |_req| json_response("HTTP/1.1 412 Precondition Failed", body))
            .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let err = client
        .update_network("br1", &serde_json::json!({}), Some("\"stale\""))
        .await
        .expect_err("412 must map to a distinguishable error");

    assert!(matches!(
        err,
        crate::Error::PreconditionFailed { resource } if resource == "br1"
    ));
}
