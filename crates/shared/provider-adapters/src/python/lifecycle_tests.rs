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
    let wheel = temporary.path().join("soma_provider.whl");
    fs::write(&wheel, b"sdk wheel").unwrap();
    let digest = Sha256::digest(b"sdk wheel")
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let spec = PythonEnvironmentSpec {
        cache_root: temporary.path().join("cache"),
        runtime: PythonRuntimeFingerprint::new("cpython", "3.12.4", "linux-x86_64").unwrap(),
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
