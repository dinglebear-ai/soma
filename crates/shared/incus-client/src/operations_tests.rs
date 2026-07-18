use super::*;
use crate::config::ClientConfig;
use crate::transport::Client;

fn success_operation_json(id: &str, status_code: u16) -> String {
    format!(
        r#"{{"type":"sync","status":"Success","status_code":200,"metadata":{{"id":"{id}","class":"task","status":"Success","status_code":{status_code},"resources":{{}},"may_cancel":false,"err":null}}}}"#
    )
}

fn failure_operation_json(id: &str) -> String {
    format!(
        r#"{{"type":"sync","status":"Success","status_code":200,"metadata":{{"id":"{id}","class":"task","status":"Failure","status_code":400,"resources":{{}},"may_cancel":false,"err":"storage pool full"}}}}"#
    )
}

fn in_progress_operation_json(id: &str) -> String {
    format!(
        r#"{{"type":"sync","status":"Success","status_code":200,"metadata":{{"id":"{id}","class":"task","status":"Running","status_code":103,"resources":{{}},"may_cancel":true,"err":null}}}}"#
    )
}

#[tokio::test]
async fn wait_for_operation_returns_the_operation_on_success() {
    let id = uuid::Uuid::new_v4();
    let body = success_operation_json(&id.to_string(), 200);
    let (socket_path, _dir) = crate::transport::unix::tests::spawn_fake_daemon(move |_req| {
        crate::transport::unix::tests::json_response("HTTP/1.1 200 OK", &body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let op = client
        .wait_for_operation(id, Some(std::time::Duration::from_secs(1)))
        .await
        .expect("success operation should be returned, not an error");

    assert_eq!(op.status_code, 200);
    assert_eq!(op.id, id);
}

#[tokio::test]
async fn wait_for_operation_returns_operation_failed_on_failure_status() {
    let id = uuid::Uuid::new_v4();
    let body = failure_operation_json(&id.to_string());
    let (socket_path, _dir) = crate::transport::unix::tests::spawn_fake_daemon(move |_req| {
        crate::transport::unix::tests::json_response("HTTP/1.1 200 OK", &body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let err = client
        .wait_for_operation(id, Some(std::time::Duration::from_secs(1)))
        .await
        .expect_err("a failure-range status_code must surface as Err, not Ok");

    match err {
        crate::Error::OperationFailed {
            id: err_id,
            status_code,
            err: message,
        } => {
            assert_eq!(err_id, id);
            assert_eq!(status_code, 400);
            assert_eq!(message.as_deref(), Some("storage pool full"));
        }
        other => panic!("expected Error::OperationFailed, got {other:?}"),
    }
}

#[tokio::test]
async fn wait_for_operation_with_explicit_timeout_returns_in_progress_without_repolling() {
    let id = uuid::Uuid::new_v4();
    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = call_count.clone();
    let body = in_progress_operation_json(&id.to_string());
    let (socket_path, _dir) = crate::transport::unix::tests::spawn_fake_daemon(move |_req| {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        crate::transport::unix::tests::json_response("HTTP/1.1 200 OK", &body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let op = client
        .wait_for_operation(id, Some(std::time::Duration::from_millis(50)))
        .await
        .expect("in-progress with an explicit timeout should return Ok, not error or hang");

    assert_eq!(op.status_code, 103);
    assert_eq!(
        call_count.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "an explicit timeout must not trigger automatic re-polling"
    );
}

#[tokio::test]
async fn wait_for_operation_with_none_timeout_repolls_until_terminal_status() {
    let id = uuid::Uuid::new_v4();
    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = call_count.clone();
    let id_string = id.to_string();
    let (socket_path, _dir) = crate::transport::unix::tests::spawn_fake_daemon(move |_req| {
        let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let body = if n == 0 {
            in_progress_operation_json(&id_string)
        } else {
            success_operation_json(&id_string, 200)
        };
        crate::transport::unix::tests::json_response("HTTP/1.1 200 OK", &body)
    })
    .await;
    let client = Client::new(ClientConfig::unix_socket(socket_path));

    let op = client
        .wait_for_operation(id, None)
        .await
        .expect("None timeout should re-poll past the in-progress response to success");

    assert_eq!(op.status_code, 200);
    assert!(
        call_count.load(std::sync::atomic::Ordering::SeqCst) >= 2,
        "expected at least one re-poll after the first in-progress response"
    );
}

#[tokio::test]
async fn wait_for_operation_is_not_bounded_by_the_client_wide_default_request_timeout() {
    // Regression test: wait_for_operation's long-poll must not inherit
    // Client::request's default per-request timeout - that default is sized
    // for ordinary, fast-returning requests, while a long-poll legitimately
    // blocks server-side for as long as the operation takes. Configure a
    // client-wide default (50ms) far shorter than how long the fake daemon
    // takes to respond (150ms) and assert the call still succeeds instead
    // of failing with Error::Timeout.
    let id = uuid::Uuid::new_v4();
    let body = success_operation_json(&id.to_string(), 200);
    let (socket_path, _dir) = crate::transport::unix::tests::spawn_fake_daemon(move |_req| {
        std::thread::sleep(std::time::Duration::from_millis(150));
        crate::transport::unix::tests::json_response("HTTP/1.1 200 OK", &body)
    })
    .await;
    let client = Client::new(
        ClientConfig::unix_socket(socket_path)
            .with_request_timeout(Some(std::time::Duration::from_millis(50))),
    );

    let op = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.wait_for_operation(id, Some(std::time::Duration::from_secs(1))),
    )
    .await
    .expect("must not hang")
    .expect(
        "a slow-but-successful long-poll must not fail with Error::Timeout just because it \
         outlives the client's unrelated default per-request timeout",
    );

    assert_eq!(op.status_code, 200);
}

#[tokio::test]
async fn cancel_operation_short_circuits_without_a_network_call_when_not_cancellable() {
    // Bind a listener but never accept a connection from it - if
    // cancel_operation made a network call here, the test would hang until
    // its own timeout rather than returning immediately.
    let dir = tempfile::tempdir().expect("create temp dir");
    let socket_path = dir.path().join("incus.sock");
    let _listener = tokio::net::UnixListener::bind(&socket_path).expect("bind");

    let client = Client::new(ClientConfig::unix_socket(socket_path));
    let id = uuid::Uuid::new_v4();

    // cancel_operation needs an Operation snapshot to check may_cancel
    // against - it takes the Operation directly rather than re-fetching by
    // id, so no network call is possible for the not-cancellable case.
    let op = Operation {
        id,
        class: OperationClass::Task,
        status: "Running".to_owned(),
        status_code: 103,
        resources: serde_json::json!({}),
        metadata: None,
        may_cancel: false,
        err: None,
    };

    let result = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        client.cancel_operation(&op),
    )
    .await
    .expect("must return immediately, not hang waiting on a network call");

    assert!(matches!(result, Err(crate::Error::NotCancellable)));
}

#[test]
fn operation_class_serde_round_trips_lowercase_wire_values() {
    for (variant, wire) in [
        (OperationClass::Task, "\"task\""),
        (OperationClass::Websocket, "\"websocket\""),
        (OperationClass::Token, "\"token\""),
    ] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, wire);
        let deserialized: OperationClass = serde_json::from_str(wire).unwrap();
        assert_eq!(deserialized, variant);
    }
}

#[test]
fn operation_class_deserializes_an_unrecognized_value_as_other_instead_of_failing() {
    // A future Incus version could add a class this crate doesn't know
    // about yet - that must not hard-fail deserialization of the whole
    // Operation, consistent with Error's own #[non_exhaustive] stance.
    let deserialized: OperationClass = serde_json::from_str("\"future-class\"").unwrap();
    assert_eq!(
        deserialized,
        OperationClass::Other("future-class".to_owned())
    );
    // Round-trips back to the original wire string, not a wrapped shape.
    let serialized = serde_json::to_string(&deserialized).unwrap();
    assert_eq!(serialized, "\"future-class\"");
}

#[test]
fn operation_deserializes_from_a_real_example_payload() {
    // Copied verbatim from the Incus REST API docs' operation object shape
    // (https://linuxcontainers.org/incus/docs/main/rest-api/), so this test
    // exercises the actual documented wire format, not a hand-constructed
    // assumption about it.
    let json = r#"{
        "id": "6916ee11-cf7f-4dd9-861f-e2ba7f4e2ea3",
        "class": "task",
        "status": "Running",
        "status_code": 103,
        "resources": {"containers": ["/1.0/containers/test"]},
        "metadata": null,
        "may_cancel": false,
        "err": ""
    }"#;
    let op: Operation = serde_json::from_str(json).expect("must deserialize");
    assert_eq!(op.class, OperationClass::Task);
    assert_eq!(op.status_code, 103);
    assert!(!op.may_cancel);
}
