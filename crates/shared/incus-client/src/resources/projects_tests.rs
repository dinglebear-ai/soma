use crate::config::ClientConfig;
use crate::transport::{
    unix::tests::{json_response, spawn_fake_daemon},
    Client, WithEtag,
};

fn project_json(name: &str) -> String {
    format!(
        r#"{{"type":"sync","status":"Success","status_code":200,"metadata":{{"name":"{name}","description":"","config":{{}}}}}}"#
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
async fn get_project_deserializes_the_documented_shape_and_returns_etag() {
    let body = project_json("default");
    let (socket_path, _dir) = spawn_fake_daemon(move |_req| {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"proj-etag\"\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        response.into_bytes()
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let WithEtag {
        value: project,
        etag,
    } = client
        .get_project("default")
        .await
        .expect("get_project should succeed");

    assert_eq!(project.name, "default");
    assert_eq!(etag.as_deref(), Some("\"proj-etag\""));
}

#[tokio::test]
async fn list_projects_requires_explicit_recursion() {
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
        .list_projects(false)
        .await
        .expect("list_projects should succeed");
    assert!(seen_request.lock().unwrap().contains("recursion=false"));
}

#[tokio::test]
async fn create_and_delete_project_are_synchronous() {
    // Real Incus (cmd/incusd/api_project.go: projectsPost, projectDelete)
    // always returns a sync response for these two endpoints - never an
    // operation - so this crate's create_project/delete_project return
    // Result<()>, not Result<Operation>.
    let (socket_path, _dir) =
        spawn_fake_daemon(move |_req| json_response("HTTP/1.1 200 OK", sync_created_json())).await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    client
        .create_project(&serde_json::json!({"name": "proj1"}))
        .await
        .expect("create_project should succeed synchronously");

    client
        .delete_project("proj1")
        .await
        .expect("delete_project should succeed synchronously");
}

#[tokio::test]
async fn update_project_sends_if_match_header_when_etag_is_provided() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        *seen.lock().unwrap() = String::from_utf8_lossy(&req).into_owned();
        json_response("HTTP/1.1 200 OK", empty_sync_json())
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    client
        .update_project(
            "proj1",
            &serde_json::json!({"description": "updated"}),
            Some("\"proj-etag\""),
        )
        .await
        .expect("update_project should succeed");

    assert!(seen_request
        .lock()
        .unwrap()
        .contains("If-Match: \"proj-etag\""));
}

#[tokio::test]
async fn update_project_guarded_derives_name_and_etag_from_a_real_fetch() {
    let seen_request = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let seen = seen_request.clone();
    let project_body = project_json("proj1");
    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = call_count.clone();
    let (socket_path, _dir) = spawn_fake_daemon(move |req| {
        let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if n == 0 {
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"real-proj-etag\"\r\nContent-Length: {}\r\n\r\n{project_body}",
                project_body.len()
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
        .get_project("proj1")
        .await
        .expect("get_project should succeed");

    client
        .update_project_guarded(&fetched, &serde_json::json!({"description": "updated"}))
        .await
        .expect("update_project_guarded should succeed");

    let request_text = seen_request.lock().unwrap().clone();
    assert!(request_text.contains("PUT /1.0/projects/proj1"));
    assert!(request_text.contains("If-Match: \"real-proj-etag\""));
}

#[tokio::test]
async fn update_project_maps_412_to_precondition_failed() {
    let body = r#"{"type":"error","error":"stale etag","error_code":412}"#;
    let (socket_path, _dir) =
        spawn_fake_daemon(move |_req| json_response("HTTP/1.1 412 Precondition Failed", body))
            .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let err = client
        .update_project("proj1", &serde_json::json!({}), Some("\"stale\""))
        .await
        .expect_err("412 must map to a distinguishable error");

    assert!(matches!(
        err,
        crate::Error::PreconditionFailed { resource } if resource == "proj1"
    ));
}
