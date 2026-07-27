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

struct UpdateUv {
    calls: Mutex<Vec<Vec<OsString>>>,
    lock_contents: String,
    fail: bool,
}

impl UpdateUv {
    fn new(lock_contents: impl Into<String>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            lock_contents: lock_contents.into(),
            fail: false,
        }
    }

    fn failing(lock_contents: impl Into<String>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            lock_contents: lock_contents.into(),
            fail: true,
        }
    }
}

impl UvRunner for UpdateUv {
    fn run(&self, _program: &Path, args: &[OsString], current_dir: &Path) -> Result<(), String> {
        self.calls.lock().unwrap().push(args.to_vec());
        if self.fail {
            return Err("simulated update failure".to_owned());
        }
        match args.first().and_then(|arg| arg.to_str()) {
            Some("lock") => fs::write(current_dir.join("uv.lock"), &self.lock_contents)
                .map_err(|error| error.to_string()),
            Some("sync") => {
                let python = current_dir.join(".venv/bin/python");
                fs::create_dir_all(python.parent().unwrap())
                    .and_then(|()| fs::write(python, "python"))
                    .map_err(|error| error.to_string())
            }
            Some("pip") => Ok(()),
            other => Err(format!("unexpected uv operation: {other:?}")),
        }
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

fn request(wheel: &Path, offline: bool) -> PythonMaterializationRequest<'_> {
    PythonMaterializationRequest {
        metadata: None,
        python_executable: Path::new("/usr/bin/python3"),
        sdk_wheel: wheel,
        offline,
    }
}

fn update_request<'a>(
    wheel: &'a Path,
    source_sha256: &'a str,
    offline: bool,
) -> PythonEnvironmentUpdateRequest<'a> {
    PythonEnvironmentUpdateRequest {
        materialization: request(wheel, offline),
        provider_source_sha256: source_sha256,
    }
}

#[test]
fn prepares_atomically_then_reuses_frozen_cache() {
    let (_temporary, plan, wheel, _digest) = fixture();
    let manager = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());
    let prepared = manager.prepare(&plan, request(&wheel, false)).unwrap();
    assert_eq!(prepared.directory, plan.directory);
    assert!(prepared.python.is_file());
    assert!(prepared.lockfile.is_file());
    assert_eq!(manager.runner.calls.lock().unwrap().len(), 3);

    let reopened = manager.open_frozen(&plan).unwrap();
    assert_eq!(reopened, prepared);
    manager.prepare(&plan, request(&wheel, true)).unwrap();
    assert_eq!(manager.runner.calls.lock().unwrap().len(), 3);
}

#[test]
fn readiness_marker_persists_plan_identity_and_lock_digest() {
    let (_temporary, plan, wheel, _digest) = fixture();
    let manager = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());

    let prepared = manager.prepare(&plan, request(&wheel, false)).unwrap();
    let marker: ReadyMarker =
        serde_json::from_slice(&fs::read(plan.directory.join(READY_FILE)).unwrap()).unwrap();
    let lock_sha256 = sha256_hex(&fs::read(&prepared.lockfile).unwrap());

    assert_eq!(marker.schema_version, READY_SCHEMA_VERSION);
    assert_eq!(marker.environment_key, plan.key);
    assert_eq!(marker.plan_version, plan.plan_version);
    assert_eq!(marker.dependency_count, plan.dependency_count);
    assert_eq!(marker.runtime, plan.runtime);
    assert_eq!(marker.sdk_wheel_tag, plan.sdk_wheel_tag);
    assert_eq!(marker.sdk_wheel_sha256, plan.sdk_wheel_sha256);
    assert_eq!(marker.uv_version, plan.uv_version);
    assert_eq!(marker.lock_sha256, lock_sha256);
    assert_eq!(prepared.key, marker.environment_key);
    assert_eq!(prepared.plan_version, marker.plan_version);
    assert_eq!(prepared.dependency_count, marker.dependency_count);
    assert_eq!(prepared.runtime, marker.runtime);
    assert_eq!(prepared.sdk_wheel_tag, marker.sdk_wheel_tag);
    assert_eq!(prepared.sdk_wheel_sha256, marker.sdk_wheel_sha256);
    assert_eq!(prepared.uv_version, marker.uv_version);
    assert_eq!(prepared.lock_sha256, marker.lock_sha256);
}

#[test]
fn tampered_lockfile_is_rejected_before_reopen() {
    let (_temporary, plan, wheel, _digest) = fixture();
    let manager = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());
    let prepared = manager.prepare(&plan, request(&wheel, false)).unwrap();
    fs::write(&prepared.lockfile, "tampered lock").unwrap();

    assert!(matches!(
        manager.open_frozen(&plan),
        Err(PythonMaterializationError::IncompleteCache(message))
            if message.contains("uv.lock digest")
    ));
}

#[test]
fn mismatched_readiness_identity_is_rejected() {
    let (_temporary, plan, wheel, _digest) = fixture();
    let manager = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());
    manager.prepare(&plan, request(&wheel, false)).unwrap();
    let marker_path = plan.directory.join(READY_FILE);
    let mut marker: ReadyMarker = serde_json::from_slice(&fs::read(&marker_path).unwrap()).unwrap();
    marker.uv_version = "unexpected-version".to_owned();
    fs::write(&marker_path, serde_json::to_vec_pretty(&marker).unwrap()).unwrap();

    assert!(matches!(
        manager.open_frozen(&plan),
        Err(PythonMaterializationError::IncompleteCache(message))
            if message.contains("does not match the plan")
    ));
}

#[test]
fn unsupported_readiness_schema_is_rejected() {
    let (_temporary, plan, wheel, _digest) = fixture();
    let manager = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());
    manager.prepare(&plan, request(&wheel, false)).unwrap();
    let marker_path = plan.directory.join(READY_FILE);
    let mut marker: ReadyMarker = serde_json::from_slice(&fs::read(&marker_path).unwrap()).unwrap();
    marker.schema_version = READY_SCHEMA_VERSION + 1;
    fs::write(&marker_path, serde_json::to_vec_pretty(&marker).unwrap()).unwrap();

    assert!(matches!(
        manager.open_frozen(&plan),
        Err(PythonMaterializationError::InvalidMarker(message))
            if message.contains("unsupported readiness schema version")
    ));
}

#[test]
fn update_prepares_immutable_candidate_and_preserves_current() {
    let (_temporary, plan, wheel, _digest) = fixture();
    let baseline = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());
    let current = baseline.prepare(&plan, request(&wheel, false)).unwrap();
    let source_sha256 = "c".repeat(64);
    let updater = PythonEnvironmentMaterializer::with_runner(
        "uv",
        UpdateUv::new("version = 2\nresolved = true\n"),
    );

    let report = updater
        .update(&plan, update_request(&wheel, &source_sha256, false))
        .unwrap();

    assert_eq!(report.outcome, PythonEnvironmentUpdateOutcome::Prepared);
    assert_eq!(report.current.as_ref(), Some(&current));
    assert_ne!(report.candidate.directory, current.directory);
    assert_eq!(report.candidate.plan_version, 3);
    assert_eq!(
        report
            .candidate
            .directory
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str()),
        Some("v3")
    );
    assert_eq!(
        report.candidate.provider_source_sha256.as_deref(),
        Some(source_sha256.as_str())
    );
    assert_eq!(
        report.candidate.input_plan_key.as_deref(),
        Some(plan.key.as_str())
    );
    assert!(current.directory.is_dir());
    assert_eq!(baseline.open_frozen(&plan).unwrap(), current);

    let calls = updater.runner.calls.lock().unwrap();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].first().and_then(|arg| arg.to_str()), Some("lock"));
    assert!(calls[0].iter().any(|arg| arg == "--upgrade"));
}

#[test]
fn identical_update_reuses_resolved_candidate() {
    let (_temporary, plan, wheel, _digest) = fixture();
    PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default())
        .prepare(&plan, request(&wheel, false))
        .unwrap();
    let source_sha256 = "c".repeat(64);
    let updater = PythonEnvironmentMaterializer::with_runner(
        "uv",
        UpdateUv::new("version = 2\nresolved = true\n"),
    );

    let first = updater
        .update(&plan, update_request(&wheel, &source_sha256, false))
        .unwrap();
    let second = updater
        .update(&plan, update_request(&wheel, &source_sha256, false))
        .unwrap();

    assert_eq!(first.outcome, PythonEnvironmentUpdateOutcome::Prepared);
    assert_eq!(second.outcome, PythonEnvironmentUpdateOutcome::Reused);
    assert_eq!(first.candidate, second.candidate);
    let calls = updater.runner.calls.lock().unwrap();
    assert_eq!(calls.len(), 4);
    assert_eq!(
        calls
            .iter()
            .filter(|args| args.first().and_then(|arg| arg.to_str()) == Some("lock"))
            .count(),
        2
    );
    assert_eq!(
        calls
            .iter()
            .filter(|args| args.first().and_then(|arg| arg.to_str()) == Some("sync"))
            .count(),
        1
    );
}

#[test]
fn source_or_resolved_lock_change_creates_distinct_candidate() {
    let (_temporary, plan, wheel, _digest) = fixture();
    PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default())
        .prepare(&plan, request(&wheel, false))
        .unwrap();
    let source_a = "c".repeat(64);
    let source_b = "d".repeat(64);
    let updater_a = PythonEnvironmentMaterializer::with_runner(
        "uv",
        UpdateUv::new("version = 2\nresolved = alpha\n"),
    );

    let first = updater_a
        .update(&plan, update_request(&wheel, &source_a, false))
        .unwrap();
    let source_changed = updater_a
        .update(&plan, update_request(&wheel, &source_b, false))
        .unwrap();
    let updater_b = PythonEnvironmentMaterializer::with_runner(
        "uv",
        UpdateUv::new("version = 2\nresolved = beta\n"),
    );
    let lock_changed = updater_b
        .update(&plan, update_request(&wheel, &source_a, false))
        .unwrap();

    assert_ne!(first.candidate.key, source_changed.candidate.key);
    assert_ne!(first.candidate.key, lock_changed.candidate.key);
    assert_ne!(source_changed.candidate.key, lock_changed.candidate.key);
    assert!(first.candidate.directory.is_dir());
    assert!(source_changed.candidate.directory.is_dir());
    assert!(lock_changed.candidate.directory.is_dir());
}

#[test]
fn failed_or_invalid_update_preserves_current_generation() {
    let (_temporary, plan, wheel, _digest) = fixture();
    let baseline = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());
    let current = baseline.prepare(&plan, request(&wheel, false)).unwrap();
    let source_sha256 = "c".repeat(64);
    let updater =
        PythonEnvironmentMaterializer::with_runner("uv", UpdateUv::failing("version = 2\n"));

    assert!(matches!(
        updater.update(&plan, update_request(&wheel, &source_sha256, false),),
        Err(PythonEnvironmentUpdateError::Uv {
            operation: "lock",
            ..
        })
    ));
    assert_eq!(baseline.open_frozen(&plan).unwrap(), current);
    let v3 = plan
        .directory
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .join("v3");
    if v3.is_dir() {
        assert!(!v3.read_dir().unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".update-")));
    }

    let invalid = PythonEnvironmentMaterializer::with_runner("uv", UpdateUv::new("version = 2\n"));
    assert!(matches!(
        invalid.update(&plan, update_request(&wheel, "bad", false)),
        Err(PythonEnvironmentUpdateError::InvalidSourceDigest)
    ));
    assert!(invalid.runner.calls.lock().unwrap().is_empty());
    assert_eq!(baseline.open_frozen(&plan).unwrap(), current);
}

#[test]
fn offline_update_forwards_offline_to_resolution_and_sync() {
    let (_temporary, plan, wheel, _digest) = fixture();
    PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default())
        .prepare(&plan, request(&wheel, false))
        .unwrap();
    let source_sha256 = "c".repeat(64);
    let updater = PythonEnvironmentMaterializer::with_runner(
        "uv",
        UpdateUv::new("version = 2\nresolved = offline\n"),
    );

    updater
        .update(&plan, update_request(&wheel, &source_sha256, true))
        .unwrap();

    let calls = updater.runner.calls.lock().unwrap();
    assert_eq!(calls.len(), 3);
    for args in &calls[..2] {
        assert!(args.iter().any(|arg| arg == "--offline"));
    }
    assert!(calls[0].iter().any(|arg| arg == "--upgrade"));
}

#[cfg(unix)]
#[test]
fn update_rejects_symlinked_candidate_root_before_uv_runs() {
    use std::os::unix::fs::symlink;

    let (temporary, plan, wheel, _digest) = fixture();
    PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default())
        .prepare(&plan, request(&wheel, false))
        .unwrap();
    let python_root = plan.directory.parent().and_then(Path::parent).unwrap();
    let outside = temporary.path().join("outside-v3");
    fs::create_dir_all(&outside).unwrap();
    symlink(&outside, python_root.join("v3")).unwrap();
    let source_sha256 = "c".repeat(64);
    let updater = PythonEnvironmentMaterializer::with_runner("uv", UpdateUv::new("version = 2\n"));

    assert!(matches!(
        updater.update(&plan, update_request(&wheel, &source_sha256, false),),
        Err(PythonEnvironmentUpdateError::UnsafeCachePath { .. })
    ));
    assert!(updater.runner.calls.lock().unwrap().is_empty());
    assert!(outside.read_dir().unwrap().next().is_none());
}

#[test]
fn repair_healthy_environment_is_a_noop() {
    let (_temporary, plan, wheel, _digest) = fixture();
    let manager = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());
    let expected = manager.prepare(&plan, request(&wheel, false)).unwrap();
    let calls = manager.runner.calls.lock().unwrap().len();

    let report = manager.repair(&plan, request(&wheel, false)).unwrap();

    assert_eq!(report.outcome, PythonEnvironmentRepairOutcome::Healthy);
    assert_eq!(report.environment, expected);
    assert!(report.replaced_error.is_none());
    assert!(report.cleanup_pending.is_none());
    assert_eq!(manager.runner.calls.lock().unwrap().len(), calls);
}

#[test]
fn repair_missing_environment_prepares_it() {
    let (_temporary, plan, wheel, _digest) = fixture();
    let manager = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());

    let report = manager.repair(&plan, request(&wheel, false)).unwrap();

    assert_eq!(report.outcome, PythonEnvironmentRepairOutcome::Prepared);
    assert!(report.environment.python.is_file());
    assert_eq!(manager.runner.calls.lock().unwrap().len(), 3);
}

#[test]
fn repair_rebuilds_corrupt_environment_atomically() {
    let (_temporary, plan, wheel, _digest) = fixture();
    let manager = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());
    let prepared = manager.prepare(&plan, request(&wheel, false)).unwrap();
    fs::write(&prepared.lockfile, "tampered lock").unwrap();

    let report = manager.repair(&plan, request(&wheel, false)).unwrap();

    assert_eq!(report.outcome, PythonEnvironmentRepairOutcome::Rebuilt);
    assert!(report
        .replaced_error
        .as_deref()
        .unwrap()
        .contains("uv.lock digest"));
    assert!(report.cleanup_pending.is_none());
    assert_eq!(manager.runner.calls.lock().unwrap().len(), 6);
    manager.open_frozen(&plan).unwrap();
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
            .contains(".repair-")));
}

#[test]
fn failed_repair_restores_original_cache_entry() {
    let (_temporary, plan, wheel, _digest) = fixture();
    fs::create_dir_all(&plan.directory).unwrap();
    let sentinel = plan.directory.join("sentinel");
    fs::write(&sentinel, "original corrupt cache").unwrap();
    let manager = PythonEnvironmentMaterializer::with_runner(
        "uv",
        FakeUv {
            calls: Mutex::new(Vec::new()),
            fail: true,
        },
    );

    assert!(matches!(
        manager.repair(&plan, request(&wheel, false)),
        Err(PythonEnvironmentRepairError::Materialization(
            PythonMaterializationError::Uv {
                operation: "lock",
                ..
            }
        ))
    ));
    assert_eq!(
        fs::read_to_string(sentinel).unwrap(),
        "original corrupt cache"
    );
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
            .contains(".repair-")));
}

#[test]
fn offline_or_invalid_sdk_repair_never_mutates_corrupt_cache() {
    let (_temporary, mut plan, wheel, _digest) = fixture();
    fs::create_dir_all(&plan.directory).unwrap();
    let sentinel = plan.directory.join("sentinel");
    fs::write(&sentinel, "preserve").unwrap();
    let manager = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());

    assert!(matches!(
        manager.repair(&plan, request(&wheel, true)),
        Err(PythonEnvironmentRepairError::Materialization(
            PythonMaterializationError::OfflineCacheMiss(_)
        ))
    ));
    assert_eq!(fs::read_to_string(&sentinel).unwrap(), "preserve");
    assert!(manager.runner.calls.lock().unwrap().is_empty());

    plan.sdk_wheel_sha256 = "b".repeat(64);
    assert!(matches!(
        manager.repair(&plan, request(&wheel, false)),
        Err(PythonEnvironmentRepairError::Materialization(
            PythonMaterializationError::SdkDigestMismatch
        ))
    ));
    assert_eq!(fs::read_to_string(sentinel).unwrap(), "preserve");
    assert!(manager.runner.calls.lock().unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn frozen_open_accepts_interpreter_symlink_but_rejects_lock_symlink() {
    use std::os::unix::fs::symlink;

    let (_temporary, plan, wheel, _digest) = fixture();
    let manager = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());
    let prepared = manager.prepare(&plan, request(&wheel, false)).unwrap();
    let managed_python = plan.directory.join("managed-python");
    fs::write(&managed_python, "python").unwrap();
    fs::remove_file(&prepared.python).unwrap();
    symlink(&managed_python, &prepared.python).unwrap();
    manager.open_frozen(&plan).unwrap();

    let lock_copy = plan.directory.join("lock-copy");
    fs::copy(&prepared.lockfile, &lock_copy).unwrap();
    fs::remove_file(&prepared.lockfile).unwrap();
    symlink(&lock_copy, &prepared.lockfile).unwrap();
    assert!(matches!(
        manager.open_frozen(&plan),
        Err(PythonMaterializationError::IncompleteCache(message))
            if message.contains("uv.lock is not a regular file")
    ));
}

#[test]
fn offline_miss_fails_without_creating_cache() {
    let (_temporary, plan, wheel, _digest) = fixture();
    let manager = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());
    assert!(matches!(
        manager.prepare(&plan, request(&wheel, true),),
        Err(PythonMaterializationError::OfflineCacheMiss(_))
    ));
    assert!(!plan.directory.exists());
    assert!(manager.runner.calls.lock().unwrap().is_empty());
}

#[test]
fn failed_uv_run_removes_owned_staging_directory() {
    let (_temporary, plan, wheel, _digest) = fixture();
    let manager = PythonEnvironmentMaterializer::with_runner(
        "uv",
        FakeUv {
            calls: Mutex::new(Vec::new()),
            fail: true,
        },
    );
    assert!(matches!(
        manager.prepare(&plan, request(&wheel, false),),
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
    let (_temporary, mut plan, wheel, _digest) = fixture();
    plan.sdk_wheel_sha256 =
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned();
    let manager = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());

    assert!(matches!(
        manager.prepare(&plan, request(&wheel, false)),
        Err(PythonMaterializationError::SdkDigestMismatch)
    ));
    assert!(manager.runner.calls.lock().unwrap().is_empty());
}

#[test]
fn rejects_existing_cache_without_readiness_marker() {
    let (_temporary, plan, wheel, _digest) = fixture();
    fs::create_dir_all(&plan.directory).unwrap();
    let manager = PythonEnvironmentMaterializer::with_runner("uv", FakeUv::default());

    assert!(matches!(
        manager.prepare(
            &plan,
            request(&wheel, false),
        ),
        Err(PythonMaterializationError::IncompleteCache(message))
            if message.contains("readiness marker")
    ));
    assert!(manager.runner.calls.lock().unwrap().is_empty());
}

#[test]
fn concurrent_incomplete_cache_preserves_cache_error() {
    let (_temporary, plan, wheel, _digest) = fixture();
    let manager = PythonEnvironmentMaterializer::with_runner(
        "uv",
        RacingUv {
            target: plan.directory.clone(),
        },
    );

    assert!(matches!(
        manager.prepare(
            &plan,
            request(&wheel, false),
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
