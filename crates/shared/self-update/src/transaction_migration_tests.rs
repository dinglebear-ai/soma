use tempfile::tempdir;

use super::*;
use crate::transaction::TestFailpoint;
use crate::{UpdateLayout, UpdatePolicy};

#[test]
fn retry_after_authority_rename_before_directory_sync_is_idempotent() {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("agent");
    let old_state = temp.path().join("old.json");
    let new_state = temp.path().join("new.json");
    std::fs::write(&executable, b"old").unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &old_state),
        UpdatePolicy::default(),
    );
    let paths = updater.validated_layout().unwrap();
    drop(updater.transaction_locks(&paths).unwrap());
    updater.set_test_failpoint(TestFailpoint::AuthorityBeforeDirectorySync);

    assert!(updater.migrate_state_file_sync(new_state.clone()).is_err());
    updater.set_test_failpoint(TestFailpoint::None);

    let migrated = updater.migrate_state_file_sync(new_state).unwrap();
    let migrated_paths = migrated.validated_layout().unwrap();
    drop(migrated.transaction_locks(&migrated_paths).unwrap());
}
