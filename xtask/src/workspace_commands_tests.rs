use super::*;

#[test]
fn help_mentions_sibling_check() {
    assert!(HELP_TEXT.contains("check-test-siblings"));
}

#[test]
fn env_report_passes_with_no_required_vars() {
    assert!(print_env_report(&[], &[("SOMA_TEST_OPTIONAL", "optional")]).is_ok());
}
