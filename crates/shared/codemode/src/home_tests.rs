use super::home::{env_non_empty, soma_home};
use serial_test::serial;

#[test]
#[serial(code_mode_soma_home)]
fn soma_home_prefers_soma_home() {
    let previous = std::env::var_os("SOMA_HOME");
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("SOMA_HOME", "/tmp/soma-home-test") };
    assert_eq!(soma_home(), std::path::PathBuf::from("/tmp/soma-home-test"));
    match previous {
        // FIXME: Audit that the environment access only happens in single-threaded code.
        Some(value) => unsafe { std::env::set_var("SOMA_HOME", value) },
        // FIXME: Audit that the environment access only happens in single-threaded code.
        None => unsafe { std::env::remove_var("SOMA_HOME") },
    }
}

#[test]
fn env_non_empty_filters_empty_values() {
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("SOMA_CODE_MODE_EMPTY_TEST", "") };
    assert_eq!(env_non_empty("SOMA_CODE_MODE_EMPTY_TEST"), None);
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("SOMA_CODE_MODE_EMPTY_TEST") };
}
