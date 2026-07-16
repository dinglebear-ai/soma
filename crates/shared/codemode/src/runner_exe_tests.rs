#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use super::runner_exe::{resolve_runner_exe, resolve_runner_exe_from};

fn expected_runner_binary_name() -> &'static str {
    if cfg!(windows) {
        "soma-codemode-runner.exe"
    } else {
        "soma-codemode-runner"
    }
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

#[test]
fn uses_current_exe_when_it_is_usable() {
    let temp = tempfile::tempdir().unwrap();
    let current = temp.path().join(expected_runner_binary_name());
    std::fs::write(&current, b"binary").unwrap();
    #[cfg(unix)]
    make_executable(&current);

    let resolved = resolve_runner_exe_from(current.clone(), None).unwrap();

    assert_eq!(resolved, current);
}

#[test]
fn finds_runner_next_to_non_runner_current_exe() {
    let temp = tempfile::tempdir().unwrap();
    let current = temp.path().join("soma");
    let runner = temp.path().join(expected_runner_binary_name());
    std::fs::write(&current, b"service").unwrap();
    std::fs::write(&runner, b"runner").unwrap();
    #[cfg(unix)]
    {
        make_executable(&current);
        make_executable(&runner);
    }

    let resolved = resolve_runner_exe_from(current, None).unwrap();

    assert_eq!(resolved, runner);
}

#[test]
fn finds_runner_above_cargo_deps_test_binary() {
    let temp = tempfile::tempdir().unwrap();
    let debug_dir = temp.path().join("debug");
    let deps_dir = debug_dir.join("deps");
    std::fs::create_dir_all(&deps_dir).unwrap();
    let current = deps_dir.join("soma_codemode_tests-1234");
    let runner = debug_dir.join(expected_runner_binary_name());
    std::fs::write(&current, b"test").unwrap();
    std::fs::write(&runner, b"runner").unwrap();
    #[cfg(unix)]
    {
        make_executable(&current);
        make_executable(&runner);
    }

    let resolved = resolve_runner_exe_from(current, None).unwrap();

    assert_eq!(resolved, runner);
}

#[test]
fn deleted_current_exe_without_override_reports_soma_guidance() {
    let err =
        resolve_runner_exe_from(PathBuf::from("/usr/local/bin/soma (deleted)"), None).unwrap_err();

    assert_eq!(err.kind(), "internal_error");
    assert!(err.user_message().contains("restart the soma service"));
    assert!(err.user_message().contains("SOMA_CODE_MODE_RUNNER_EXE"));
}

#[test]
fn override_must_be_absolute() {
    let err = resolve_runner_exe_from(
        PathBuf::from("/usr/local/bin/soma"),
        Some(PathBuf::from("target/debug/soma-codemode-runner")),
    )
    .unwrap_err();

    assert_eq!(err.kind(), "invalid_param");
    assert!(err.user_message().contains("absolute path"));
}

#[test]
fn missing_override_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing-runner");

    let err =
        resolve_runner_exe_from(PathBuf::from("/usr/local/bin/soma"), Some(missing)).unwrap_err();

    assert_eq!(err.kind(), "internal_error");
    assert!(err.user_message().contains("SOMA_CODE_MODE_RUNNER_EXE"));
    assert!(err.user_message().contains("missing-runner"));
}

#[test]
fn explicit_override_is_used_after_validation() {
    let temp = tempfile::tempdir().unwrap();
    let override_path = temp.path().join("soma-codemode-runner");
    std::fs::write(&override_path, b"binary").unwrap();
    #[cfg(unix)]
    make_executable(&override_path);

    let resolved = resolve_runner_exe_from(
        PathBuf::from("/usr/local/bin/soma (deleted)"),
        Some(override_path.clone()),
    )
    .unwrap();

    assert_eq!(resolved, std::fs::canonicalize(override_path).unwrap());
}

#[cfg(unix)]
#[test]
fn non_executable_override_is_rejected() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let override_path = temp.path().join("soma-codemode-runner");
    std::fs::write(&override_path, b"binary").unwrap();
    let mut perms = std::fs::metadata(&override_path).unwrap().permissions();
    perms.set_mode(0o644);
    std::fs::set_permissions(&override_path, perms).unwrap();

    let err = resolve_runner_exe_from(PathBuf::from("/usr/local/bin/soma"), Some(override_path))
        .unwrap_err();

    assert_eq!(err.kind(), "internal_error");
    assert!(err.user_message().contains("not executable"));
}

#[cfg(unix)]
#[test]
fn symlink_to_bad_target_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let link = temp.path().join("runner-link");
    std::os::unix::fs::symlink(temp.path().join("missing-target"), &link).unwrap();

    let err =
        resolve_runner_exe_from(PathBuf::from("/usr/local/bin/soma"), Some(link)).unwrap_err();

    assert_eq!(err.kind(), "internal_error");
    assert!(err.user_message().contains("cannot be resolved"));
}

#[cfg(unix)]
#[test]
fn override_rejects_group_or_world_writable_file() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let override_path = temp.path().join("soma-codemode-runner");
    std::fs::write(&override_path, b"binary").unwrap();
    let mut perms = std::fs::metadata(&override_path).unwrap().permissions();
    perms.set_mode(0o777);
    std::fs::set_permissions(&override_path, perms).unwrap();

    let err = resolve_runner_exe_from(PathBuf::from("/usr/local/bin/soma"), Some(override_path))
        .unwrap_err();

    assert_eq!(err.kind(), "internal_error");
    assert!(err.user_message().contains("group/world writable"));
}

#[test]
fn old_lab_env_name_is_not_preferred() {
    let _guard = env_lock().lock().unwrap();
    let legacy_env = concat!("LAB", "BY_CODE_MODE_RUNNER_EXE");
    let temp = tempfile::tempdir().unwrap();
    let current = temp.path().join(expected_runner_binary_name());
    let legacy = temp.path().join("legacy-runner");
    std::fs::write(&current, b"current").unwrap();
    std::fs::write(&legacy, b"legacy").unwrap();
    #[cfg(unix)]
    {
        make_executable(&current);
        make_executable(&legacy);
    }
    std::env::remove_var("SOMA_CODE_MODE_RUNNER_EXE");
    std::env::set_var(legacy_env, &legacy);

    let resolved = resolve_runner_exe_from(current.clone(), None).unwrap();

    std::env::remove_var(legacy_env);
    assert_eq!(resolved, current);
}

#[test]
fn env_override_success_uses_soma_name() {
    let _guard = env_lock().lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let override_path = temp.path().join("env-runner");
    std::fs::write(&override_path, b"binary").unwrap();
    #[cfg(unix)]
    make_executable(&override_path);
    std::env::set_var("SOMA_CODE_MODE_RUNNER_EXE", &override_path);

    let resolved = resolve_runner_exe().unwrap();

    std::env::remove_var("SOMA_CODE_MODE_RUNNER_EXE");
    assert_eq!(resolved, std::fs::canonicalize(override_path).unwrap());
}
