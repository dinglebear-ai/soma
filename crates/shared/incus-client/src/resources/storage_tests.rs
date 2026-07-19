use crate::config::ClientConfig;
use crate::transport::{
    unix::tests::{json_response, spawn_fake_daemon},
    Client,
};

fn pool_json(name: &str) -> String {
    format!(
        r#"{{"type":"sync","status":"Success","status_code":200,"metadata":{{"name":"{name}","driver":"zfs","status":"Created","config":{{}}}}}}"#
    )
}

/// `recursion=false` shape: Incus returns bare volume URL strings, not
/// typed objects.
fn volume_list_urls_json() -> &'static str {
    r#"{"type":"sync","status":"Success","status_code":200,"metadata":["/1.0/storage-pools/default/volumes/custom/vol1"]}"#
}

/// `recursion=true` shape: Incus returns full volume objects.
fn volume_list_objects_json() -> &'static str {
    r#"{"type":"sync","status":"Success","status_code":200,"metadata":[{"name":"vol1","type":"custom","content_type":"filesystem","config":{}}]}"#
}

fn operation_json(id: &str) -> String {
    format!(
        r#"{{"type":"async","status":"Operation created","status_code":100,"operation":"/1.0/operations/{id}","metadata":{{"id":"{id}","class":"task","status":"Running","status_code":103,"resources":{{}},"may_cancel":true,"err":null}}}}"#
    )
}

fn volume_json(name: &str) -> String {
    format!(
        r#"{{"type":"sync","status":"Success","status_code":200,"metadata":{{"name":"{name}","type":"custom","content_type":"filesystem","config":{{}}}}}}"#
    )
}

/// The real Incus daemon's `SyncResponseLocation(true, nil, ...)` shape for
/// pool/project/network create endpoints - `metadata` is JSON `null`.
fn sync_created_json() -> &'static str {
    r#"{"type":"sync","status":"Success","status_code":200,"metadata":null}"#
}

/// The real Incus daemon's `EmptySyncResponse` shape for update/delete
/// endpoints - `metadata` is an empty object.
fn empty_sync_json() -> &'static str {
    r#"{"type":"sync","status":"Success","status_code":200,"metadata":{}}"#
}

#[tokio::test]
async fn get_storage_pool_deserializes_the_documented_shape_and_returns_etag() {
    let body = pool_json("default");
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"pool-etag\"\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        response.into_bytes()
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let fetched = client
        .get_storage_pool("default")
        .await
        .expect("get_storage_pool should succeed");
    assert_eq!(fetched.value().name, "default");
    assert_eq!(fetched.value().driver, "zfs");
    assert_eq!(fetched.etag(), Some("\"pool-etag\""));
}

#[tokio::test]
async fn get_storage_pool_maps_404_to_not_found() {
    let body = r#"{"type":"error","error":"not found","error_code":404}"#;
    let (socket_path, _dir) =
        spawn_fake_daemon(move |_req| json_response("HTTP/1.1 404 Not Found", body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let err = client
        .get_storage_pool("missing")
        .await
        .expect_err("404 must map to a distinguishable error, not the generic Error::Api");

    assert!(matches!(
        err,
        crate::Error::NotFound { resource } if resource == "missing"
    ));
}

/// Mocks a fake daemon that returns the Incus-documented shape for each
/// recursion level: a bare URL-string array for `recursion=false`, full
/// volume objects for `recursion=true`. Real callers must be able to
/// deserialize both, since `list_storage_volumes` returns untyped
/// `serde_json::Value`s (see the P1 fix this test guards against: this
/// method used to be typed `Vec<StorageVolume>`, which panics on the
/// `recursion=false` wire shape).
async fn spawn_recursion_aware_volume_daemon() -> (std::path::PathBuf, tempfile::TempDir) {
    spawn_fake_daemon(move |req| {
        let req_text = String::from_utf8_lossy(&req);
        if req_text.contains("recursion=true") {
            json_response("HTTP/1.1 200 OK", volume_list_objects_json())
        } else {
            json_response("HTTP/1.1 200 OK", volume_list_urls_json())
        }
    })
    .await
}

#[tokio::test]
async fn list_storage_volumes_is_scoped_to_a_specific_pool() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        *seen.lock().unwrap() = String::from_utf8_lossy(&req).into_owned();
        json_response("HTTP/1.1 200 OK", volume_list_objects_json())
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let volumes = client
        .list_storage_volumes("default", true)
        .await
        .expect("list_storage_volumes should succeed");

    assert_eq!(volumes.len(), 1);
    assert_eq!(volumes[0]["name"], "vol1");
    assert!(seen_request
        .lock()
        .unwrap()
        .contains("/1.0/storage-pools/default/volumes"));
}

#[tokio::test]
async fn list_storage_volumes_with_recursion_false_deserializes_bare_url_strings() {
    let (socket_path, _dir) = spawn_recursion_aware_volume_daemon().await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let volumes = client
        .list_storage_volumes("default", false)
        .await
        .expect("recursion=false must deserialize as bare URL strings, not typed objects");

    assert_eq!(volumes.len(), 1);
    assert_eq!(
        volumes[0],
        serde_json::json!("/1.0/storage-pools/default/volumes/custom/vol1")
    );
}

#[tokio::test]
async fn list_storage_volumes_with_recursion_true_deserializes_full_objects() {
    let (socket_path, _dir) = spawn_recursion_aware_volume_daemon().await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let volumes = client
        .list_storage_volumes("default", true)
        .await
        .expect("recursion=true must deserialize as full volume objects");

    assert_eq!(volumes.len(), 1);
    assert_eq!(volumes[0]["name"], "vol1");
    assert_eq!(volumes[0]["type"], "custom");
}

#[tokio::test]
async fn create_and_delete_storage_pool_are_synchronous() {
    // Real Incus (cmd/incusd/storage_pools.go: storagePoolsPost,
    // storagePoolDelete) always returns a sync response for these two
    // endpoints - never an operation - so this crate's
    // create_storage_pool/delete_storage_pool return Result<()>, not
    // Result<Operation>.
    let (socket_path, _dir) =
        spawn_fake_daemon(move |_req| json_response("HTTP/1.1 200 OK", sync_created_json())).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    client
        .create_storage_pool(&serde_json::json!({"name": "pool1", "driver": "zfs"}))
        .await
        .expect("create_storage_pool should succeed synchronously");

    client
        .delete_storage_pool("pool1")
        .await
        .expect("delete_storage_pool should succeed synchronously");
}

#[tokio::test]
async fn update_storage_pool_sends_if_match_header_when_etag_is_provided() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        *seen.lock().unwrap() = String::from_utf8_lossy(&req).into_owned();
        json_response("HTTP/1.1 200 OK", empty_sync_json())
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    client
        .update_storage_pool(
            "pool1",
            &serde_json::json!({"description": "updated"}),
            Some("\"pool-etag\""),
        )
        .await
        .expect("update_storage_pool should succeed synchronously");

    assert!(seen_request
        .lock()
        .unwrap()
        .contains("If-Match: \"pool-etag\""));
}

#[tokio::test]
async fn update_storage_pool_maps_412_to_precondition_failed() {
    let body = r#"{"type":"error","error":"stale etag","error_code":412}"#;
    let (socket_path, _dir) =
        spawn_fake_daemon(move |_req| json_response("HTTP/1.1 412 Precondition Failed", body))
            .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let err = client
        .update_storage_pool("pool1", &serde_json::json!({}), Some("\"stale\""))
        .await
        .expect_err("412 must map to a distinguishable error");

    assert!(matches!(
        err,
        crate::Error::PreconditionFailed { resource } if resource == "pool1"
    ));
}

#[tokio::test]
async fn update_storage_pool_guarded_derives_name_and_etag_from_a_real_fetch() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let pool_body = pool_json("pool1");
    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = call_count.clone();
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if n == 0 {
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"real-pool-etag\"\r\nContent-Length: {}\r\n\r\n{pool_body}",
                pool_body.len()
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
        .get_storage_pool("pool1")
        .await
        .expect("get_storage_pool should succeed");

    client
        .update_storage_pool_guarded(&fetched, &serde_json::json!({"description": "updated"}))
        .await
        .expect("update_storage_pool_guarded should succeed");

    let request_text = seen_request.lock().unwrap().clone();
    assert!(request_text.contains("PUT /1.0/storage-pools/pool1"));
    assert!(request_text.contains("If-Match: \"real-pool-etag\""));
}

#[tokio::test]
async fn get_storage_volume_deserializes_the_documented_shape_and_returns_etag() {
    let body = volume_json("vol1");
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"vol-etag\"\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        response.into_bytes()
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let fetched = client
        .get_storage_volume("default", "custom", "vol1")
        .await
        .expect("get_storage_volume should succeed");
    assert_eq!(fetched.value().name, "vol1");
    assert_eq!(fetched.value().volume_type, "custom");
    assert_eq!(fetched.etag(), Some("\"vol-etag\""));
}

#[tokio::test]
async fn create_storage_volume_without_a_source_is_synchronous() {
    // Real Incus (cmd/incusd/storage_volumes.go: doVolumeCreateOrCopy)
    // returns EmptySyncResponse when the request has no source.name - a
    // blank-volume create - so create_storage_volume returns Ok(None).
    let (socket_path, _dir) =
        spawn_fake_daemon(move |_req| json_response("HTTP/1.1 200 OK", sync_created_json())).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let op = client
        .create_storage_volume(
            "default",
            &serde_json::json!({"name": "vol1", "type": "custom"}),
        )
        .await
        .expect("create_storage_volume without a source should succeed synchronously");

    assert!(
        op.is_none(),
        "a blank volume create has no operation to wait on"
    );
}

#[tokio::test]
async fn create_storage_volume_from_a_copy_is_asynchronous() {
    // Real Incus returns operations.OperationResponse when the request has
    // a source.name (copying another volume) - genuinely long-running, so
    // create_storage_volume returns Ok(Some(operation)) in that case.
    let id = uuid::Uuid::new_v4().to_string();
    let body = operation_json(&id);
    let (socket_path, _dir) =
        spawn_fake_daemon(move |_req| json_response("HTTP/1.1 202 Accepted", &body)).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let op = client
        .create_storage_volume(
            "default",
            &serde_json::json!({
                "name": "vol2",
                "type": "custom",
                "source": {"name": "vol1", "pool": "default"}
            }),
        )
        .await
        .expect("create_storage_volume from a copy should succeed asynchronously")
        .expect("a copy operation must return Some(Operation) to wait on");

    assert_eq!(op.id.to_string(), id);
}

#[tokio::test]
async fn update_storage_volume_sends_if_match_header_when_etag_is_provided() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        *seen.lock().unwrap() = String::from_utf8_lossy(&req).into_owned();
        json_response("HTTP/1.1 200 OK", empty_sync_json())
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    client
        .update_storage_volume(
            "default",
            "custom",
            "vol1",
            &serde_json::json!({"config": {}}),
            Some("\"vol-etag\""),
        )
        .await
        .expect("update_storage_volume should succeed synchronously");

    let request_text = seen_request.lock().unwrap().clone();
    assert!(request_text.contains("/1.0/storage-pools/default/volumes/custom/vol1"));
    assert!(request_text.contains("If-Match: \"vol-etag\""));
}

#[tokio::test]
async fn update_storage_volume_maps_412_to_precondition_failed() {
    let body = r#"{"type":"error","error":"stale etag","error_code":412}"#;
    let (socket_path, _dir) =
        spawn_fake_daemon(move |_req| json_response("HTTP/1.1 412 Precondition Failed", body))
            .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let err = client
        .update_storage_volume(
            "default",
            "custom",
            "vol1",
            &serde_json::json!({}),
            Some("\"stale\""),
        )
        .await
        .expect_err("412 must map to a distinguishable error");

    assert!(matches!(
        err,
        crate::Error::PreconditionFailed { resource } if resource == "vol1"
    ));
}

#[tokio::test]
async fn update_storage_volume_guarded_derives_type_name_and_etag_from_a_real_fetch() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let volume_body = volume_json("vol1");
    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = call_count.clone();
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if n == 0 {
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"real-vol-etag\"\r\nContent-Length: {}\r\n\r\n{volume_body}",
                volume_body.len()
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
        .get_storage_volume("default", "custom", "vol1")
        .await
        .expect("get_storage_volume should succeed");

    client
        .update_storage_volume_guarded("default", &fetched, &serde_json::json!({"config": {}}))
        .await
        .expect("update_storage_volume_guarded should succeed");

    let request_text = seen_request.lock().unwrap().clone();
    assert!(request_text.contains("PUT /1.0/storage-pools/default/volumes/custom/vol1"));
    assert!(request_text.contains("If-Match: \"real-vol-etag\""));
}
