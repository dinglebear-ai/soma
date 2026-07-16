use super::redact;

#[test]
fn redaction_module_is_reexported() {
    assert!(redact::is_sensitive_key("authorization"));
}
