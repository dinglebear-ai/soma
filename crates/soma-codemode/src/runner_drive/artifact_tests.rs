use super::artifact::artifact_call;

#[test]
fn artifact_call_uses_reserved_namespace() {
    assert_eq!(artifact_call("a.txt").id, "artifact::write");
}
