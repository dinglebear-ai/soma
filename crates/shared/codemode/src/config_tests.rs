use super::config::*;

#[test]
fn defaults_validate_and_use_soma_env_names() {
    let config = CodeModeConfig::default();
    assert!(!config.enabled);
    assert!(config.validate().is_ok());
    assert_eq!(SERVICE, "code_mode");
    assert_eq!(MAX_SOURCE_BYTES, 20_000);
}

#[test]
fn call_budget_env_is_capped() {
    std::env::set_var("SOMA_CODE_MODE_MAX_CALLS_PER_RUN", "9000");
    assert_eq!(max_calltool_per_run(), 2048);
    std::env::remove_var("SOMA_CODE_MODE_MAX_CALLS_PER_RUN");
}
