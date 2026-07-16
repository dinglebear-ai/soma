use super::checks::{check_port_available, check_required_var};

#[test]
fn check_required_var_passes_when_value_is_set() {
    let result = check_required_var("SOMA_API_KEY", "sk-test-value");
    assert!(result.ok, "non-empty value should pass");
}

#[test]
fn check_required_var_fails_when_value_is_empty() {
    let result = check_required_var("SOMA_API_KEY", "");
    assert!(!result.ok, "empty value should fail");
    assert!(result.hint.is_some(), "failed check should have a hint");
}

#[test]
fn check_port_available_passes_for_unused_high_port() {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("should bind to an ephemeral port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let result = check_port_available("127.0.0.1", port);
    assert!(result.ok, "unused high port should be available");
}
