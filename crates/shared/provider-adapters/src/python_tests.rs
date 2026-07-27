use std::path::PathBuf;

use crate::python_protocol::PYTHON_PROTOCOL_HEADROOM_BYTES;

use super::{
    environment::{PythonRuntimeFingerprint, PythonWheelTag},
    materializer::PreparedPythonEnvironment,
    protocol_output_limit, select_python_command, PythonInterpreter,
};

#[test]
fn default_python_command_matches_platform_launcher() {
    #[cfg(windows)]
    assert_eq!(super::default_python_command(), "python");

    #[cfg(not(windows))]
    assert_eq!(super::default_python_command(), "python3");

    assert_eq!(
        select_python_command(None, None, &PythonInterpreter::Ambient),
        super::default_python_command()
    );
}

#[test]
fn prepared_interpreter_uses_materialized_python() {
    let python = PathBuf::from("cache")
        .join(".venv")
        .join("bin")
        .join("python");
    let environment = PreparedPythonEnvironment {
        key: "environment-key".to_owned(),
        directory: PathBuf::from("cache"),
        python: python.clone(),
        lockfile: PathBuf::from("cache").join("uv.lock"),
        plan_version: 2,
        dependency_count: 0,
        runtime: PythonRuntimeFingerprint::new(
            "cpython",
            "3.12.4",
            "linux-x86_64",
            "manylinux_2_17_x86_64",
        )
        .unwrap(),
        sdk_wheel_tag: PythonWheelTag {
            python: "cp311".to_owned(),
            abi: "abi3".to_owned(),
            platform: "manylinux_2_17_x86_64".to_owned(),
        },
        sdk_wheel_sha256: "a".repeat(64),
        uv_version: "0.11.31".to_owned(),
        lock_sha256: "b".repeat(64),
        provider_source_sha256: None,
        input_plan_key: None,
    };

    let interpreter = PythonInterpreter::prepared(&environment);

    assert_eq!(interpreter, PythonInterpreter::Prepared(python.clone()));
    assert_eq!(
        select_python_command(None, None, &interpreter),
        python.to_string_lossy()
    );
}

#[test]
fn configured_commands_override_prepared_interpreter() {
    let interpreter = PythonInterpreter::Prepared(PathBuf::from("prepared-python"));

    assert_eq!(
        select_python_command(
            Some("manifest-python"),
            Some("environment-python".to_owned()),
            &interpreter,
        ),
        "manifest-python"
    );
    assert_eq!(
        select_python_command(None, Some("environment-python".to_owned()), &interpreter),
        "environment-python"
    );
}

#[test]
fn protocol_capture_limit_adds_bounded_headroom() {
    assert_eq!(
        protocol_output_limit(256 * 1024),
        256 * 1024 + PYTHON_PROTOCOL_HEADROOM_BYTES
    );
    assert_eq!(protocol_output_limit(usize::MAX), usize::MAX);
}
