use super::status_body;

#[test]
fn status_body_contains_only_local_runtime_metadata() {
    let body = status_body("test-soma");

    assert_eq!(body["status"], "ok");
    assert_eq!(body["server"], "test-soma");
    assert_eq!(body["transport"], "http");
    assert!(body.get("api_url").is_none());
}
