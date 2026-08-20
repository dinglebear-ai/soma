use super::*;

#[test]
fn rejects_invalid_and_protected_env_names() {
    assert_eq!(
        validate_env_name("lowercase").unwrap_err(),
        EnvPolicyError::InvalidName
    );
    assert_eq!(
        validate_env_name("LD_PRELOAD").unwrap_err(),
        EnvPolicyError::ProtectedName
    );
    for name in [
        "PATH",
        "HOME",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
        "NODE_OPTIONS",
        "PYTHONPATH",
        "PYTHONHOME",
        "MCP_GATEWAY_TOKEN",
    ] {
        assert_eq!(
            validate_env_name(name).unwrap_err(),
            EnvPolicyError::ProtectedName,
            "expected {name} to be protected"
        );
    }
    validate_env_name("UPSTREAM_TOKEN").unwrap();
}

#[test]
fn rejects_control_characters_in_env_values() {
    for value in ["line\nbreak", "carriage\rreturn", "nul\0byte"] {
        assert_eq!(
            validate_env_value(value).unwrap_err(),
            EnvPolicyError::InvalidValue
        );
    }
    validate_env_value("ordinary secret value").unwrap();
}

#[test]
fn split_secret_flags_are_redacted_for_logs() {
    let args = vec![
        "--api-key".to_owned(),
        "secret".to_owned(),
        "--name".to_owned(),
        "demo".to_owned(),
    ];
    let redacted = redact_spawn_args_for_log(&args);
    assert_eq!(redacted[1], "[redacted]");
    assert_eq!(redacted[3], "demo");
}

#[test]
fn remote_args_cannot_disable_spawn_guard() {
    let args = vec!["--disable-spawn-guard".to_owned()];
    assert_eq!(
        reject_spawn_guard_overrides(&args).unwrap_err(),
        EnvPolicyError::SpawnGuardOverride
    );
}
