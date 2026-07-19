use std::path::{Path, PathBuf};

use super::artifacts::ensure_no_recovery_artifacts;
use super::authority::{
    AuthorityWriteOutcome, read_state_authority_unconfirmed, rewrite_state_authority,
};
use super::transaction_io::{suffix_path, sync_parent};
use crate::{MigrationOutcome, Result, UpdateError, UpdateLayout, Updater, bind_state_identity};

impl Updater {
    pub(super) fn migrate_state_file_sync(
        &self,
        new_state_file: PathBuf,
    ) -> Result<MigrationOutcome> {
        let old = self.validated_layout()?;
        let new_state = bind_state_identity(&new_state_file)
            .map_err(|error| UpdateError::io(&new_state_file, error))?;
        let migrated = Updater::new(
            UpdateLayout::new(&old.executable, &new_state),
            self.policy().clone(),
        );
        migrated.ensure_layout_bound()?;
        let new = migrated.validated_layout()?;
        if old.executable != new.executable
            || old.authority != new.authority
            || old.authority_temp != new.authority_temp
        {
            return Err(UpdateError::InvalidPolicy(
                "state migration must retain the executable identity",
            ));
        }
        validate_migration_namespace(&old, &new)?;

        let mut lock_paths = old.locks.clone();
        lock_paths.extend(new.locks.iter().cloned());
        lock_paths.sort();
        lock_paths.dedup();
        let _locks = self.acquire_transaction_locks(&lock_paths)?;
        let authority = read_state_authority_unconfirmed(&old.authority, &old.authority_temp)?;
        let authority_state = match authority {
            Some(bound) if bound == old.state => AuthorityState::Current,
            Some(bound) if bound == new.state => AuthorityState::Migrated,
            Some(bound) => {
                return Err(UpdateError::InvalidLayout {
                    first: bound,
                    second: old.state,
                });
            }
            None => AuthorityState::Absent,
        };
        if authority_state == AuthorityState::Migrated {
            return Ok(match sync_parent(&old.authority) {
                Ok(()) => MigrationOutcome::Migrated { updater: migrated },
                Err(error) => MigrationOutcome::MigratedIndeterminate {
                    updater: migrated,
                    diagnostic: error.to_string(),
                },
            });
        }
        ensure_absent(&old.state, "the current transaction marker exists")?;
        ensure_absent(
            &suffix_path(&old.state, ".tmp"),
            "the current marker temporary file exists",
        )?;
        ensure_absent(&new.state, "the destination transaction marker exists")?;
        ensure_absent(
            &suffix_path(&new.state, ".tmp"),
            "the destination marker temporary file exists",
        )?;
        ensure_no_recovery_artifacts(&old.executable)?;
        if authority_state == AuthorityState::Current {
            sync_parent(&old.authority)?;
        }
        if authority_state == AuthorityState::Absent {
            match rewrite_state_authority(self, &old.authority, &old.authority_temp, &old.state)? {
                AuthorityWriteOutcome::Durable => {}
                AuthorityWriteOutcome::RenamedIndeterminate(error) => return Err(error),
            }
        }
        if old.state == new.state {
            return Ok(MigrationOutcome::Migrated { updater: migrated });
        }
        Ok(
            match rewrite_state_authority(self, &old.authority, &old.authority_temp, &new.state)? {
                AuthorityWriteOutcome::Durable => MigrationOutcome::Migrated { updater: migrated },
                AuthorityWriteOutcome::RenamedIndeterminate(error) => {
                    MigrationOutcome::MigratedIndeterminate {
                        updater: migrated,
                        diagnostic: error.to_string(),
                    }
                }
            },
        )
    }
}

fn validate_migration_namespace(
    old: &super::transaction_layout::LayoutPaths,
    new: &super::transaction_layout::LayoutPaths,
) -> Result<()> {
    validate_marker_namespace(old, old, true)?;
    validate_marker_namespace(new, new, true)?;
    if old.state != new.state {
        validate_marker_namespace(old, new, false)?;
        validate_marker_namespace(new, old, false)?;
    }
    Ok(())
}

fn validate_marker_namespace(
    marker_layout: &super::transaction_layout::LayoutPaths,
    protected_layout: &super::transaction_layout::LayoutPaths,
    allow_matching_marker_namespace: bool,
) -> Result<()> {
    let marker_temp = suffix_path(&marker_layout.state, ".tmp");
    let protected_temp = suffix_path(&protected_layout.state, ".tmp");
    for marker in [&marker_layout.state, &marker_temp] {
        for protected in protected_layout
            .protected
            .iter()
            .map(PathBuf::as_path)
            .chain(std::iter::once(protected_temp.as_path()))
        {
            if paths_may_alias(marker, protected) {
                let matching_namespace = allow_matching_marker_namespace
                    && ((marker == &marker_layout.state && protected == protected_layout.state)
                        || (marker == &marker_temp && protected == protected_temp));
                if !matching_namespace {
                    return Err(UpdateError::InvalidLayout {
                        first: marker.clone(),
                        second: protected.to_path_buf(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn paths_may_alias(first: &Path, second: &Path) -> bool {
    if first == second {
        return true;
    }

    if let (Ok(first_metadata), Ok(second_metadata)) =
        (std::fs::metadata(first), std::fs::metadata(second))
    {
        use std::os::unix::fs::MetadataExt;
        if first_metadata.dev() == second_metadata.dev()
            && first_metadata.ino() == second_metadata.ino()
        {
            return true;
        }
    }

    let same_canonical_path = match (
        std::fs::canonicalize(first),
        std::fs::canonicalize(second),
    ) {
        (Ok(first_canonical), Ok(second_canonical)) => first_canonical == second_canonical,
        _ => false,
    };
    if same_canonical_path {
        return true;
    }

    unresolved_leaves_may_alias(first, second)
}

fn unresolved_leaves_may_alias(first: &Path, second: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;

    let (Some(first_parent), Some(second_parent), Some(first_leaf), Some(second_leaf)) = (
        first.parent(),
        second.parent(),
        first.file_name(),
        second.file_name(),
    ) else {
        return false;
    };
    if !parents_share_identity(first_parent, second_parent) {
        return false;
    }

    let first_bytes = first_leaf.as_bytes();
    let second_bytes = second_leaf.as_bytes();
    if first_bytes == second_bytes {
        return true;
    }
    if first_bytes.is_ascii() && second_bytes.is_ascii() {
        return first_bytes.eq_ignore_ascii_case(second_bytes);
    }

    // Portable normalization and full case-fold behavior varies by filesystem.
    // Refuse differing non-ASCII or invalid UTF-8 leaves rather than probing the
    // directory and creating a lock before their identity is known.
    true
}

fn parents_share_identity(first: &Path, second: &Path) -> bool {
    let same_canonical_path = match (
        std::fs::canonicalize(first),
        std::fs::canonicalize(second),
    ) {
        (Ok(first), Ok(second)) => first == second,
        _ => false,
    };
    if same_canonical_path {
        return true;
    }

    match (std::fs::metadata(first), std::fs::metadata(second)) {
        (Ok(first), Ok(second)) => metadata_identity_matches(&first, &second),
        _ => false,
    }
}

fn metadata_identity_matches(first: &std::fs::Metadata, second: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    first.dev() == second.dev() && first.ino() == second.ino()
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum AuthorityState {
    Absent,
    Current,
    Migrated,
}

fn ensure_absent(path: &Path, message: &str) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Err(UpdateError::StateMigrationBlocked {
            path: path.to_path_buf(),
            message: message.into(),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(UpdateError::io(path, error)),
    }
}

#[cfg(test)]
#[path = "transaction_migration_tests.rs"]
mod tests;
