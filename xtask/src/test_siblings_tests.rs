use super::*;

#[test]
fn gateway_src_root_is_checked() {
    assert!(crate_src_roots()
        .iter()
        .any(|path| path == &PathBuf::from("crates/soma-gateway/src")));
}

#[test]
fn expected_sibling_uses_source_stem() {
    let source = PathBuf::from("crates/soma-gateway/src/config.rs");
    assert_eq!(
        expected_test_sibling(&source),
        PathBuf::from("crates/soma-gateway/src/config_tests.rs")
    );
}

#[test]
fn matching_source_strips_tests_suffix() {
    let tests = PathBuf::from("crates/soma-gateway/src/config_tests.rs");
    assert_eq!(
        matching_source(&tests),
        PathBuf::from("crates/soma-gateway/src/config.rs")
    );
}
