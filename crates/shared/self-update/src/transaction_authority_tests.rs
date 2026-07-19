use tempfile::tempdir;

use super::*;
use crate::{UpdateLayout, UpdatePolicy};

#[test]
fn partial_write_is_recovered_without_truncating_stable_lock() {
    authority_write_failure_recovers(TestFailpoint::AuthorityAfterPartialWrite, false);
}

#[test]
fn file_sync_failure_is_recovered_from_temporary_record() {
    authority_write_failure_recovers(TestFailpoint::AuthorityBeforeFileSync, false);
}

#[test]
fn directory_sync_failure_leaves_a_complete_recoverable_record() {
    authority_write_failure_recovers(TestFailpoint::AuthorityBeforeDirectorySync, true);
}

#[test]
fn authority_requires_exact_mode_without_special_bits() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let executable = temp.path().join("agent");
    let state = temp.path().join("update.json");
    std::fs::write(&executable, b"old").unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let paths = updater.validated_layout().unwrap();
    drop(updater.transaction_locks(&paths).unwrap());
    std::fs::set_permissions(&paths.authority, std::fs::Permissions::from_mode(0o4600)).unwrap();

    assert!(matches!(
        read_state_authority(&paths.authority, &paths.authority_temp),
        Err(UpdateError::InvalidMarker { path, .. }) if path == paths.authority
    ));
}

fn authority_write_failure_recovers(failpoint: TestFailpoint, renamed: bool) {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("agent");
    let state = temp.path().join("update.json");
    std::fs::write(&executable, b"old").unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let paths = updater.validated_layout().unwrap();
    updater.set_test_failpoint(failpoint);

    assert!(updater.transaction_locks(&paths).is_err());
    assert!(std::fs::read(&paths.executable_lock).unwrap().is_empty());
    assert_eq!(paths.authority.exists(), renamed);
    assert_eq!(paths.authority_temp.exists(), !renamed);

    updater.set_test_failpoint(TestFailpoint::None);
    drop(updater.transaction_locks(&paths).unwrap());
    assert!(paths.authority.exists());
    assert!(!paths.authority_temp.exists());
    let reconstructed = Updater::new(
        UpdateLayout::new(&executable, &state),
        UpdatePolicy::default(),
    );
    let reconstructed_paths = reconstructed.validated_layout().unwrap();
    drop(
        reconstructed
            .transaction_locks(&reconstructed_paths)
            .unwrap(),
    );
}
