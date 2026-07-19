use tempfile::tempdir;

use super::*;
use crate::transaction::TestFailpoint;
use crate::{InstallOutcome, UpdateDirective, UpdateLayout, UpdatePolicy};

fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

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

    let outcome = updater.migrate_state_file_sync(new_state.clone()).unwrap();
    let migrated = match outcome {
        MigrationOutcome::MigratedIndeterminate {
            updater,
            diagnostic,
        } => {
            assert!(!diagnostic.is_empty());
            updater
        }
        MigrationOutcome::Migrated { .. } => panic!("expected indeterminate migration"),
    };
    updater.set_test_failpoint(TestFailpoint::None);

    let migrated_paths = migrated.validated_layout().unwrap();
    drop(migrated.transaction_locks(&migrated_paths).unwrap());
    assert!(matches!(
        updater.migrate_state_file_sync(new_state).unwrap(),
        MigrationOutcome::Migrated { .. }
    ));
}

#[tokio::test]
async fn retry_after_indeterminate_migration_preserves_new_pending_marker() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let executable = temp.path().join("agent");
    let old_state = temp.path().join("old.json");
    let new_state = temp.path().join("new.json");
    std::fs::write(&executable, b"#!/bin/sh\necho 'agent 1.0.0'\n").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &old_state),
        UpdatePolicy::default(),
    );
    let paths = updater.validated_layout().unwrap();
    drop(updater.transaction_locks(&paths).unwrap());
    updater.set_test_failpoint(TestFailpoint::AuthorityBeforeDirectorySync);

    let migrated = match updater.migrate_state_file_sync(new_state.clone()).unwrap() {
        MigrationOutcome::MigratedIndeterminate { updater, .. } => updater,
        MigrationOutcome::Migrated { .. } => panic!("expected indeterminate migration"),
    };
    updater.set_test_failpoint(TestFailpoint::None);

    let replacement = b"#!/bin/sh\necho 'agent 2.0.0'\n";
    let directive = UpdateDirective::new("2.0.0", "/binary", digest(replacement)).unwrap();
    let staged = migrated.stage(&replacement[..], &directive).await.unwrap();
    let validated = migrated.validate(staged).await.unwrap();
    assert!(matches!(
        migrated.install(validated, "1.0.0").await.unwrap(),
        InstallOutcome::RestartRequired { .. }
    ));
    let pending_marker = std::fs::read(&new_state).unwrap();

    assert!(matches!(
        updater.migrate_state_file_sync(new_state).unwrap(),
        MigrationOutcome::Migrated { .. }
    ));
    assert_eq!(
        std::fs::read(migrated.layout().state_file()).unwrap(),
        pending_marker
    );
    assert!(!old_state.exists());
}

#[test]
fn old_marker_collision_with_new_lock_is_rejected_before_side_effects() {
    migration_collision_is_side_effect_free("destination.json.lock", "destination.json");
}

#[test]
fn new_marker_collision_with_old_lock_is_rejected_before_side_effects() {
    migration_collision_is_side_effect_free("current.json", "current.json.lock");
}

#[test]
fn case_variant_absent_markers_are_rejected_before_side_effects() {
    migration_collision_is_side_effect_free("Update.JSON", "update.json");
}

#[test]
fn unicode_full_casefold_candidates_are_rejected_before_side_effects() {
    migration_collision_is_side_effect_free("Straße.json", "STRASSE.json");
}

#[test]
fn unicode_sigma_candidates_are_rejected_before_side_effects() {
    migration_collision_is_side_effect_free("σ.json", "ς.json");
}

#[test]
fn unicode_normalization_candidates_are_rejected_before_side_effects() {
    migration_collision_is_side_effect_free("é.json", "e\u{301}.json");
}

#[test]
fn non_utf8_alias_check_is_side_effect_free() {
    use std::os::unix::ffi::OsStringExt;

    let temp = tempdir().unwrap();
    let first = temp
        .path()
        .join(std::ffi::OsString::from_vec(b"update-\xff.json".to_vec()));
    let second = temp
        .path()
        .join(std::ffi::OsString::from_vec(b"update-\xfe.json".to_vec()));

    assert!(unresolved_leaves_may_alias(&first, &second));
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn filesystem_identical_parent_metadata_is_recognized() {
    let temp = tempdir().unwrap();
    let other = temp.path().join("other");
    std::fs::create_dir(&other).unwrap();
    let first = std::fs::metadata(temp.path()).unwrap();
    let same = std::fs::metadata(temp.path()).unwrap();
    let distinct = std::fs::metadata(&other).unwrap();

    assert!(metadata_identity_matches(&first, &same));
    assert!(!metadata_identity_matches(&first, &distinct));
}

#[test]
fn aliased_parent_collision_check_is_side_effect_free() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let state = temp.path().join("state");
    let alias = temp.path().join("state-alias");
    std::fs::create_dir(&state).unwrap();
    symlink(&state, &alias).unwrap();

    assert!(unresolved_leaves_may_alias(
        &state.join("Update.JSON"),
        &alias.join("update.json")
    ));
    assert_eq!(std::fs::read_dir(&state).unwrap().count(), 0);
}

fn migration_collision_is_side_effect_free(
    old_name: impl AsRef<std::ffi::OsStr>,
    new_name: impl AsRef<std::ffi::OsStr>,
) {
    let temp = tempdir().unwrap();
    let executable = temp.path().join("agent");
    let old_state = temp.path().join(old_name.as_ref());
    let new_state = temp.path().join(new_name.as_ref());
    std::fs::write(&executable, b"old").unwrap();
    let updater = Updater::new(
        UpdateLayout::new(&executable, &old_state),
        UpdatePolicy::default(),
    );

    let result = updater.migrate_state_file_sync(new_state);
    assert!(
        matches!(result, Err(UpdateError::InvalidLayout { .. })),
        "unexpected migration result: {result:?}"
    );
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
}
