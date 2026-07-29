use super::config::*;
use serial_test::serial;

#[test]
fn defaults_validate_and_use_soma_env_names() {
    let config = CodeModeConfig::default();
    assert!(!config.enabled);
    assert!(config.validate().is_ok());
    assert_eq!(SERVICE, "code_mode");
    assert_eq!(MAX_SOURCE_BYTES, 20_000);
}

#[test]
#[serial(code_mode_call_budget_env)]
fn call_budget_env_is_capped() {
    let previous = std::env::var_os("SOMA_CODE_MODE_MAX_CALLS_PER_RUN");
    // FIXME: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("SOMA_CODE_MODE_MAX_CALLS_PER_RUN", "9000") };
    assert_eq!(max_calltool_per_run(), 2048);
    match previous {
        // FIXME: Audit that the environment access only happens in single-threaded code.
        Some(value) => unsafe { std::env::set_var("SOMA_CODE_MODE_MAX_CALLS_PER_RUN", value) },
        // FIXME: Audit that the environment access only happens in single-threaded code.
        None => unsafe { std::env::remove_var("SOMA_CODE_MODE_MAX_CALLS_PER_RUN") },
    }
}
