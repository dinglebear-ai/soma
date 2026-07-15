use super::path_safety::reject_path_traversal;

#[test]
fn rejects_traversal_and_reserved_state_dirs() {
    assert!(reject_path_traversal("../secret").is_err());
    assert!(reject_path_traversal("safe/.soma-state/file").is_err());
    assert!(reject_path_traversal(&format!("safe/{}/file", concat!(".la", "bby-state"))).is_err());
    assert!(reject_path_traversal("safe/file.txt").is_ok());
}
