use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use super::*;
use sha2::{Digest, Sha256};

#[derive(Clone, Default)]
struct RecordingUv {
    calls: Arc<Mutex<Vec<Vec<OsString>>>>,
    projects: Arc<Mutex<Vec<String>>>,
}

impl UvRunner for RecordingUv {
    fn run(&self, _program: &Path, args: &[OsString], current_dir: &Path) -> Result<(), String> {
        self.calls.lock().unwrap().push(args.to_vec());
        match args.first().and_then(|arg| arg.to_str()) {
            Some("lock") => {
                self.projects
                    .lock()
                    .unwrap()
                    .push(fs::read_to_string(current_dir.join("pyproject.toml")).unwrap());
                fs::write(current_dir.join("uv.lock"), "version = 1").unwrap();
            }
            Some("sync") => {
                let python = current_dir.join(if cfg!(windows) {
                    ".venv/Scripts/python.exe"
                } else {
                    ".venv/bin/python"
                });
                fs::create_dir_all(python.parent().unwrap()).unwrap();
                fs::write(python, "python").unwrap();
            }
            Some("pip") => {}
            other => return Err(format!("unexpected uv operation: {other:?}")),
        }
        Ok(())
    }
}

fn fixture() -> (
    tempfile::TempDir,
    PythonEnvironmentSpec,
    RecordingUv,
    PathBuf,
) {
    let temporary = tempfile::tempdir().unwrap();
    let wheel = temporary
        .path()
        .join("soma_provider-0.2.0-cp311-abi3-manylinux_2_17_x86_64.whl");
    fs::write(&wheel, b"sdk wheel").unwrap();
    let digest = Sha256::digest(b"sdk wheel")
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let spec = PythonEnvironmentSpec {
        cache_root: temporary.path().join("cache"),
        runtime: PythonRuntimeFingerprint::new(
            "cpython",
            "3.12.4",
            "linux-x86_64",
            "manylinux_2_17_x86_64",
        )
        .unwrap(),
        python_executable: PathBuf::from("/usr/bin/python3"),
        sdk_wheel: wheel,
        sdk_wheel_sha256: digest,
        uv_version: "0.11.31".to_owned(),
        offline: false,
    };
    let provider = temporary.path().join("provider.py");
    let runner = RecordingUv::default();
    (temporary, spec, runner, provider)
}

#[test]
fn prepares_provider_without_pep723_metadata() {
    let (_temporary, spec, runner, provider) = fixture();
    fs::write(
        &provider,
        "PROVIDER = {'name': 'plain', 'kind': 'python'}\n",
    )
    .unwrap();
    let lifecycle = PythonEnvironmentLifecycle::with_runner("uv", spec, runner.clone());

    let prepared = lifecycle.prepare_provider(&provider).unwrap();

    assert!(prepared.python.is_file());
    assert!(prepared.lockfile.is_file());
    let projects = runner.projects.lock().unwrap();
    let project: toml::Value = toml::from_str(&projects[0]).unwrap();
    assert_eq!(
        project["project"]["dependencies"].as_array().unwrap().len(),
        0
    );
}

#[test]
fn passes_pep723_metadata_into_materialization() {
    let (_temporary, spec, runner, provider) = fixture();
    fs::write(
        &provider,
        r#"# /// script
# requires-python = ">=3.11"
# dependencies = ["anyio>=4"]
#
# [tool.uv]
# prerelease = "disallow"
# ///
PROVIDER = {"name": "pep", "kind": "python"}
"#,
    )
    .unwrap();
    let lifecycle = PythonEnvironmentLifecycle::with_runner("uv", spec, runner.clone());

    lifecycle.prepare_provider(&provider).unwrap();

    let projects = runner.projects.lock().unwrap();
    let project: toml::Value = toml::from_str(&projects[0]).unwrap();
    assert_eq!(
        project["project"]["requires-python"].as_str(),
        Some(">=3.11")
    );
    assert_eq!(
        project["project"]["dependencies"][0].as_str(),
        Some("anyio>=4")
    );
    assert_eq!(
        project["tool"]["uv"]["prerelease"].as_str(),
        Some("disallow")
    );
}

#[test]
fn incompatible_candidate_fails_before_uv_runs() {
    let (_temporary, spec, runner, provider) = fixture();
    fs::write(
        &provider,
        r#"# /// script
# requires-python = ">=3.13"
# ///
PROVIDER = {"name": "incompatible", "kind": "python"}
"#,
    )
    .unwrap();
    let lifecycle = PythonEnvironmentLifecycle::with_runner("uv", spec, runner.clone());

    assert!(matches!(
        lifecycle.prepare_provider(&provider),
        Err(PythonEnvironmentLifecycleError::Environment(
            PythonEnvironmentError::IncompatiblePython { .. }
        ))
    ));
    assert!(runner.calls.lock().unwrap().is_empty());
}

#[test]
fn warm_cache_reuse_never_invokes_uv() {
    let (_temporary, spec, runner, provider) = fixture();
    fs::write(&provider, "PROVIDER = {'name': 'warm', 'kind': 'python'}\n").unwrap();
    let lifecycle = PythonEnvironmentLifecycle::with_runner("uv", spec, runner.clone());

    let first = lifecycle.prepare_provider(&provider).unwrap();
    let calls_after_first_prepare = runner.calls.lock().unwrap().len();
    let second = lifecycle.prepare_provider(&provider).unwrap();

    assert_eq!(first, second);
    assert_eq!(calls_after_first_prepare, 3);
    assert_eq!(
        runner.calls.lock().unwrap().len(),
        calls_after_first_prepare
    );
}

#[derive(Clone, Copy)]
struct RejectingUv;

impl UvRunner for RejectingUv {
    fn run(&self, _program: &Path, _args: &[OsString], _current_dir: &Path) -> Result<(), String> {
        Err("uv must not run for a complete frozen cache".to_owned())
    }
}

#[test]
fn offline_restart_reopens_complete_cache_without_uv() {
    let (_temporary, mut spec, runner, provider) = fixture();
    fs::write(
        &provider,
        "PROVIDER = {'name': 'offline', 'kind': 'python'}\n",
    )
    .unwrap();
    let online = PythonEnvironmentLifecycle::with_runner("uv", spec.clone(), runner);
    let expected = online.prepare_provider(&provider).unwrap();

    spec.offline = true;
    let restarted = PythonEnvironmentLifecycle::with_runner("uv", spec, RejectingUv);
    let prepared = restarted.prepare_provider(&provider).unwrap();

    assert_eq!(prepared, expected);
}
