use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use super::*;
use crate::python::environment::{plan_python_environment, PythonRuntimeFingerprint};

#[derive(Default)]
struct FakeUv {
    calls: Mutex<Vec<Vec<OsString>>>,
    fail: bool,
}

impl UvRunner for FakeUv {
    fn run(&self, _program: &Path, args: &[OsString], current_dir: &Path) -> Result<(), String> {
        self.calls.lock().unwrap().push(args.to_vec());
        if self.fail {
            return Err("simulated failure".to_owned());
        }
        simulate_uv(args, current_dir);
        Ok(())
    }
}

struct RacingUv {
    target: PathBuf,
}

impl UvRunner for RacingUv {
    fn run(&self, _program: &Path, args: &[OsString], current_dir: &Path) -> Result<(), String> {
        simulate_uv(args, current_dir);
        if args.first().and_then(|arg| arg.to_str()) == Some("pip") {
            fs::create_dir_all(&self.target).unwrap();
            fs::write(self.target.join("occupied"), "racing writer").unwrap();
        }
        Ok(())
    }
}

fn simulate_uv(args: &[OsString], current_dir: &Path) {
    match args.first().and_then(|arg| arg.to_str()) {
        Some("lock") => fs::write(current_dir.join("uv.lock"), "version = 1").unwrap(),
        Some("sync") => {
            let python = current_dir.join(".venv/bin/python");
            fs::create_dir_all(python.parent().unwrap()).unwrap();
            fs::write(python, "python").unwrap();
        }
        Some("pip") => {}
        _ => panic!("unexpected uv call"),
    }
}

fn fixture() -> (tempfile::TempDir, PythonEnvironmentPlan, PathBuf, String) {
    let temporary = tempfile::tempdir().unwrap();
    let wheel = temporary
        .path()
        .join("soma_provider-0.2.0-cp311-abi3-manylinux_2_17_x86_64.whl");
    fs::write(&wheel, b"sdk wheel").unwrap();
    let digest = sha256_hex(b"sdk wheel");
    let runtime =
        PythonRuntimeFingerprint::new("cpython", "3.12.4", "linux-x86_64", "manylinux_2_17_x86_64")
            .unwrap();
    let plan = plan_python_environment(
        &temporary.path().join("cache"),
        None,
        &runtime,
        &wheel,
        &digest,
        "0.11.31",
    )
    .unwrap();
    (temporary, plan, wheel, digest)
}

fn request<'a>(
    wheel: &'a Path,
    digest: &'a str,
    offline: bool,
) -> PythonMaterializationRequest<'a> {
    PythonMaterializationRequest {
        metadata: None,
        python_executable: Path::new("/usr/bin/python3"),
        sdk_wheel: wheel,
        sdk_wheel_sha256: digest,
        uv_version: "0.11.31",
        offline,
    }
}

#[test]
fn prepares_atomically_then_reuses_frozen_cache() {
    let (_temporary, plan, wheel, digest) = fixture();
    let manager = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());
    let prepared = manager
        .prepare(&plan, request(&wheel, &digest, false))
        .unwrap();
    assert_eq!(prepared.directory, plan.directory);
    assert!(prepared.python.is_file());
    assert!(prepared.lockfile.is_file());
    assert_eq!(manager.runner.calls.lock().unwrap().len(), 3);

    let reopened = manager.open_frozen(&plan).unwrap();
    assert_eq!(reopened, prepared);
    manager
        .prepare(&plan, request(&wheel, &digest, true))
        .unwrap();
    assert_eq!(manager.runner.calls.lock().unwrap().len(), 3);
}

#[test]
fn offline_miss_fails_without_creating_cache() {
    let (_temporary, plan, wheel, digest) = fixture();
    let manager = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());
    assert!(matches!(
        manager.prepare(&plan, request(&wheel, &digest, true),),
        Err(PythonMaterializationError::OfflineCacheMiss(_))
    ));
    assert!(!plan.directory.exists());
    assert!(manager.runner.calls.lock().unwrap().is_empty());
}

#[test]
fn failed_uv_run_removes_owned_staging_directory() {
    let (_temporary, plan, wheel, digest) = fixture();
    let manager = PythonEnvironmentMaterializer::with_runner(
        "uv",
        FakeUv {
            calls: Mutex::new(Vec::new()),
            fail: true,
        },
    );
    assert!(matches!(
        manager.prepare(&plan, request(&wheel, &digest, false),),
        Err(PythonMaterializationError::Uv {
            operation: "lock",
            ..
        })
    ));
    assert!(!plan.directory.exists());
    let parent = plan.directory.parent().unwrap();
    assert!(fs::read_dir(parent).unwrap().next().is_none());
}

#[test]
fn rejects_wrong_sdk_before_running_uv() {
    let (_temporary, plan, wheel, _digest) = fixture();
    let manager = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());
    assert!(matches!(
        manager.prepare(
            &plan,
            request(
                &wheel,
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                false,
            ),
        ),
        Err(PythonMaterializationError::SdkDigestMismatch)
    ));
    assert!(manager.runner.calls.lock().unwrap().is_empty());
}

#[test]
fn rejects_existing_cache_without_readiness_marker() {
    let (_temporary, plan, wheel, digest) = fixture();
    fs::create_dir_all(&plan.directory).unwrap();
    let manager = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());

    assert!(matches!(
        manager.prepare(
            &plan,
            request(&wheel, &digest, false),
        ),
        Err(PythonMaterializationError::IncompleteCache(message))
            if message.contains("readiness marker")
    ));
    assert!(manager.runner.calls.lock().unwrap().is_empty());
}

#[test]
fn concurrent_incomplete_cache_preserves_cache_error() {
    let (_temporary, plan, wheel, digest) = fixture();
    let manager = PythonEnvironmentMaterializer::with_runner(
        "uv",
        RacingUv {
            target: plan.directory.clone(),
        },
    );

    assert!(matches!(
        manager.prepare(
            &plan,
            request(&wheel, &digest, false),
        ),
        Err(PythonMaterializationError::IncompleteCache(message))
            if message.contains("readiness marker")
    ));
    assert!(!plan
        .directory
        .parent()
        .unwrap()
        .read_dir()
        .unwrap()
        .any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with('.')));
}

#[test]
fn renders_normalized_dependencies_and_uv_settings() {
    let metadata = Pep723Metadata {
        requires_python: Some(">=3.11".to_owned()),
        dependencies: vec!["anyio>=4".to_owned()],
        uv: Some(toml::Value::Table(toml::Table::from_iter([(
            "prerelease".to_owned(),
            toml::Value::String("disallow".to_owned()),
        )]))),
    };
    let project = render_project(Some(&metadata));
    let value: toml::Value = toml::from_str(&project).unwrap();
    assert_eq!(value["project"]["requires-python"].as_str(), Some(">=3.11"));
    assert_eq!(
        value["project"]["dependencies"][0].as_str(),
        Some("anyio>=4")
    );
    assert_eq!(value["tool"]["uv"]["prerelease"].as_str(), Some("disallow"));
}
