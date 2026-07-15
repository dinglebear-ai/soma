use super::local::is_local_provider;

#[test]
fn detects_local_provider_namespaces() {
    assert!(is_local_provider("state::read"));
    assert!(is_local_provider("git::status"));
    assert!(!is_local_provider("github::list"));
}
