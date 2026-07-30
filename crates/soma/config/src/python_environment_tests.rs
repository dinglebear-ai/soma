use super::python_environment::PythonEnvironmentConfig;

fn enabled() -> PythonEnvironmentConfig {
    PythonEnvironmentConfig {
        enabled: true,
        cache_root: "/var/cache/soma".to_owned(),
        uv_program: "/usr/local/bin/uv".to_owned(),
        uv_version: "0.11.31".to_owned(),
        python_executable: "/usr/bin/python3".to_owned(),
        runtime_implementation: "cpython".to_owned(),
        runtime_version: "3.12.4".to_owned(),
        runtime_platform: "linux-x86_64".to_owned(),
        wheel_platform_tag: "manylinux_2_17_x86_64".to_owned(),
        sdk_wheel: "/opt/soma/soma_provider-0.2.0-cp38-abi3-manylinux_2_17_x86_64.whl".to_owned(),
        sdk_wheel_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_owned(),
        ..PythonEnvironmentConfig::default()
    }
}

#[test]
fn update_and_offline_are_mutually_exclusive() {
    let mut config = enabled();
    config.offline = true;
    config.update = true;
    assert!(
        config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("cannot be enabled")
    );
}

#[test]
fn policy_version_must_be_positive() {
    let mut config = enabled();
    config.policy_version = 0;
    assert!(
        config
            .validate()
            .unwrap_err()
            .to_string()
            .contains("POLICY_VERSION")
    );
}
