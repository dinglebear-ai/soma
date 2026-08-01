use std::os::unix::fs::PermissionsExt;

use super::*;

#[test]
fn runtime_directories_are_owner_only() {
    let directory = secure_runtime_subdir("test-runtime").unwrap();
    let mode = std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);
}

#[test]
fn runtime_component_validation_fails_closed() {
    assert!(secure_runtime_subdir("../escape").is_err());
    assert!(secure_runtime_subdir("UpperCase").is_err());
}

#[test]
fn preexisting_symlink_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    std::fs::create_dir(&target).unwrap();
    let link = temp.path().join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let uid = rustix::process::getuid().as_raw();
    assert!(secure_directory(&link, uid).is_err());
}
