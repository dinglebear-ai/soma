use std::path::PathBuf;

use crate::python_protocol::PYTHON_PROTOCOL_HEADROOM_BYTES;

use super::{
    materializer::PreparedPythonEnvironment, protocol_output_limit, select_python_command,
    PythonInterpreter,
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
        directory: PathBuf::from("cache"),
        python: python.clone(),
        lockfile: PathBuf::from("cache").join("uv.lock"),
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
