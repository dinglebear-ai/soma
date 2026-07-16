use super::preamble::{generate_local_provider_js, tool_name_to_snake};

#[test]
fn preamble_reexports_helpers() {
    assert_eq!(tool_name_to_snake("a.b"), "a_b");
    assert!(generate_local_provider_js().contains("codemode.git"));
}
