use super::*;

#[test]
fn disabled_environment_does_not_install_operator_port() {
    let port = python_environment_port(&Config::default()).expect("disabled config is valid");
    assert!(port.is_none());
}

#[tokio::test]
async fn blocking_task_failures_use_public_operator_error_contract() {
    let error = python_environment_blocking::<(), _>("inventory", || {
        Err(python_environment_port_error(
            "inventory",
            "token=private-value",
        ))
    })
    .await
    .expect_err("operation should fail");

    assert_eq!(error.code, "python_environment_operation_failed");
    let public = soma_application::ApplicationError::from(error);
    assert_eq!(public.message, "[redacted provider diagnostic]");
    assert!(!public.message.contains("private-value"));
}
