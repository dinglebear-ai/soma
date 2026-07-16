use super::home::{env_non_empty, soma_home};

#[test]
fn soma_home_prefers_soma_home() {
    std::env::set_var("SOMA_HOME", "/tmp/soma-home-test");
    assert_eq!(soma_home(), std::path::PathBuf::from("/tmp/soma-home-test"));
    std::env::remove_var("SOMA_HOME");
}

#[test]
fn env_non_empty_filters_empty_values() {
    std::env::set_var("SOMA_CODE_MODE_EMPTY_TEST", "");
    assert_eq!(env_non_empty("SOMA_CODE_MODE_EMPTY_TEST"), None);
    std::env::remove_var("SOMA_CODE_MODE_EMPTY_TEST");
}
