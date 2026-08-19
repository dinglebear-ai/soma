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

#[test]
#[serial(code_mode_artifact_env)]
fn artifact_limits_support_config_env_precedence_and_disable_zero() {
    let vars = [
        "SOMA_CODE_MODE_ARTIFACT_MAX_MIB",
        "SOMA_CODE_MODE_ARTIFACT_RETENTION_RUNS",
        "SOMA_CODE_MODE_ARTIFACT_MAX_STORE_MIB",
    ];
    let previous: Vec<_> = vars
        .iter()
        .map(|name| (*name, std::env::var_os(name)))
        .collect();
    for name in vars {
        // SAFETY: this test is serialized with every other artifact-env test.
        unsafe { std::env::remove_var(name) };
    }

    let config = CodeModeConfig {
        artifact_max_mib: Some(3),
        artifact_retention_runs: Some(17),
        artifact_max_store_mib: Some(23),
        ..CodeModeConfig::default()
    };
    assert_eq!(effective_artifact_max_bytes(&config), 3 * 1024 * 1024);
    assert_eq!(effective_artifact_retention_runs(&config), 17);
    assert_eq!(
        effective_artifact_max_store_bytes(&config),
        23 * 1024 * 1024
    );

    // SAFETY: this test is serialized with every other artifact-env test.
    unsafe {
        std::env::set_var("SOMA_CODE_MODE_ARTIFACT_MAX_MIB", "5");
        std::env::set_var("SOMA_CODE_MODE_ARTIFACT_RETENTION_RUNS", "0");
        std::env::set_var("SOMA_CODE_MODE_ARTIFACT_MAX_STORE_MIB", "0");
    }
    assert_eq!(effective_artifact_max_bytes(&config), 5 * 1024 * 1024);
    assert_eq!(effective_artifact_retention_runs(&config), 0);
    assert_eq!(effective_artifact_max_store_bytes(&config), 0);

    for (name, value) in previous {
        // SAFETY: this test is serialized with every other artifact-env test.
        match value {
            Some(value) => unsafe { std::env::set_var(name, value) },
            None => unsafe { std::env::remove_var(name) },
        }
    }
}
