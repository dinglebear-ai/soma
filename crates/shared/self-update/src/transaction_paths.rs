use std::path::{Path, PathBuf};

use crate::{Result, UpdateError};

pub(crate) fn validate_distinct_paths(paths: &[PathBuf]) -> Result<()> {
    for (index, first) in paths.iter().enumerate() {
        for second in &paths[index + 1..] {
            if paths_may_alias(first, second) {
                return Err(UpdateError::InvalidLayout {
                    first: first.clone(),
                    second: second.clone(),
                });
            }
        }
    }
    Ok(())
}

pub(super) fn paths_may_alias(first: &Path, second: &Path) -> bool {
    if first == second {
        return true;
    }

    if let (Ok(first_metadata), Ok(second_metadata)) =
        (std::fs::metadata(first), std::fs::metadata(second))
        && metadata_identity_matches(&first_metadata, &second_metadata)
    {
        return true;
    }

    let same_canonical_path = match (
        std::fs::canonicalize(first),
        std::fs::canonicalize(second),
    ) {
        (Ok(first_canonical), Ok(second_canonical)) => first_canonical == second_canonical,
        _ => false,
    };
    same_canonical_path || unresolved_leaves_may_alias(first, second)
}

pub(super) fn unresolved_leaves_may_alias(first: &Path, second: &Path) -> bool {
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

pub(super) fn metadata_identity_matches(
    first: &std::fs::Metadata,
    second: &std::fs::Metadata,
) -> bool {
    use std::os::unix::fs::MetadataExt;

    first.dev() == second.dev() && first.ino() == second.ino()
}

#[cfg(test)]
#[path = "transaction_paths_tests.rs"]
mod tests;
