use super::internal::is_internal_call;

#[test]
fn detects_internal_namespace() {
    assert!(is_internal_call("__soma_internal::describe"));
    assert!(!is_internal_call("git::status"));
}
