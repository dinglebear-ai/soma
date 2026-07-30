use std::{fs, path::PathBuf};

use crate::python_protocol::PYTHON_PROTOCOL_HEADROOM_BYTES;
use serde_json::json;
use soma_provider_core::validate_provider_manifest_value;

use super::{
    PythonInterpreter, PythonProvider,
    environment::{PythonRuntimeFingerprint, PythonWheelTag},
    materializer::PreparedPythonEnvironment,
    protocol_output_limit, select_python_command,
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
fn configured_commands_cannot_bypass_prepared_interpreter() {
    let interpreter = PythonInterpreter::Prepared(PathBuf::from("prepared-python"));

    assert_eq!(
        select_python_command(
            Some("manifest-python"),
            Some("environment-python".to_owned()),
            &interpreter,
        ),
        "prepared-python"
    );
    assert_eq!(
        select_python_command(None, Some("environment-python".to_owned()), &interpreter),
        "prepared-python"
    );
    assert_eq!(
        select_python_command(
            Some("manifest-python"),
            Some("environment-python".to_owned()),
            &PythonInterpreter::Ambient,
        ),
        "manifest-python"
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

#[test]
fn persistent_mode_rejects_provider_and_tool_runtime_environment_requirements() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("provider.py");
    fs::write(&path, "PROVIDER = {'name': 'env-test', 'kind': 'python'}\n").unwrap();
    for manifest in [
        json!({
            "schema_version": 1,
            "provider": {"name": "env-test", "kind": "python"},
            "env": [{"name": "TOKEN", "required": false}],
            "tools": []
        }),
        json!({
            "schema_version": 1,
            "provider": {"name": "env-test", "kind": "python"},
            "tools": [{
                "name": "run",
                "description": "run",
                "input_schema": {"type": "object"},
                "env": [{"name": "TOKEN", "required": false}]
            }]
        }),
    ] {
        let catalog = validate_provider_manifest_value(&manifest).unwrap();
        let error = match PythonProvider::new_persistent(
            path.clone(),
            catalog,
            "SOMA",
            PythonInterpreter::Ambient,
            Default::default(),
        ) {
            Ok(_) => panic!("environment requirement must fail closed"),
            Err(error) => error,
        };
        assert_eq!(error.code.as_ref(), "python_persistent_env_unsupported");
    }
}

#[test]
fn persistent_identity_changes_with_the_complete_immutable_generation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("provider.py");
    fs::write(
        &path,
        "PROVIDER = {'name': 'generation-test', 'kind': 'python'}\n",
    )
    .unwrap();
    let catalog = validate_provider_manifest_value(&json!({
        "schema_version": 1,
        "provider": {"name": "generation-test", "kind": "python"},
        "tools": []
    }))
    .unwrap();
    let first = PythonProvider::arc_persistent_in_generation(
        path.clone(),
        catalog.clone(),
        "SOMA",
        PythonInterpreter::Ambient,
        Default::default(),
        "a".repeat(64),
    )
    .unwrap();
    let second = PythonProvider::arc_persistent_in_generation(
        path,
        catalog,
        "SOMA",
        PythonInterpreter::Ambient,
        Default::default(),
        "b".repeat(64),
    )
    .unwrap();

    let first = soma_provider_core::Provider::runtime_status(first.as_ref()).unwrap();
    let second = soma_provider_core::Provider::runtime_status(second.as_ref()).unwrap();
    assert_ne!(first["generation_id"], second["generation_id"]);
}

#[test]
fn persistent_generation_digest_is_validated_without_panicking() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("provider.py");
    fs::write(
        &path,
        "PROVIDER = {'name': 'generation-test', 'kind': 'python'}\n",
    )
    .unwrap();
    let catalog = validate_provider_manifest_value(&json!({
        "schema_version": 1,
        "provider": {"name": "generation-test", "kind": "python"},
        "tools": []
    }))
    .unwrap();
    for invalid in ["short", &"g".repeat(64)] {
        let error = match PythonProvider::arc_persistent_in_generation(
            path.clone(),
            catalog.clone(),
            "SOMA",
            PythonInterpreter::Ambient,
            Default::default(),
            invalid.to_owned(),
        ) {
            Ok(_) => panic!("invalid generation digest must fail closed"),
            Err(error) => error,
        };
        assert_eq!(error.code.as_ref(), "python_generation_digest_invalid");
    }
}
