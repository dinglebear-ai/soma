use super::*;

#[test]
fn api_error_display_includes_status_and_message() {
    let err = Error::Api {
        status_code: 404,
        message: "not found".to_owned(),
    };
    let text = err.to_string();
    assert!(text.contains("404"));
    assert!(text.contains("not found"));
}

#[test]
fn operation_failed_display_falls_back_when_err_is_none() {
    let id = uuid::Uuid::nil();
    let err = Error::OperationFailed {
        id,
        status_code: 400,
        err: None,
    };
    assert!(err.to_string().contains("no error message"));
}

#[test]
fn operation_failed_display_includes_err_when_present() {
    let id = uuid::Uuid::nil();
    let err = Error::OperationFailed {
        id,
        status_code: 400,
        err: Some("storage pool full".to_owned()),
    };
    assert!(err.to_string().contains("storage pool full"));
}

#[test]
fn serialization_error_converts_via_from() {
    let json_err = serde_json::from_str::<serde_json::Value>("{not json")
        .expect_err("deliberately malformed JSON");
    let err: Error = json_err.into();
    assert!(matches!(err, Error::Serialization(_)));
}

#[test]
fn io_error_converts_via_from() {
    let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "boom");
    let err: Error = io_err.into();
    assert!(matches!(err, Error::Transport(_)));
}
