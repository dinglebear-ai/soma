use soma_service::provider_errors::redact_public;
use soma_service::ProviderError;

#[test]
fn provider_errors_redact_secret_bearing_diagnostics() {
    let error = ProviderError::execution(
        "demo",
        "leaky",
        "stderr: Authorization: Bearer sk-secret token=abc body: private",
    );

    assert_eq!(&*error.code, "provider_execution_failed");
    assert_eq!(&*error.message, "[redacted provider diagnostic]");
    assert!(!format!("{error}").contains("sk-secret"));
}

#[test]
fn public_redaction_handles_common_secret_markers() {
    for sample in [
        "cookie: session=secret",
        "api_key=secret",
        "password=secret",
        "set-cookie: secret",
        "body: upstream private payload",
    ] {
        assert_eq!(redact_public(sample), "[redacted provider diagnostic]");
    }
}
