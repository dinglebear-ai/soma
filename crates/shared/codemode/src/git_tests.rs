use super::git::safety::validate_ref;

#[test]
fn git_root_reexports_safety_surface() {
    assert!(validate_ref("main").is_ok());
}
