use super::*;

#[test]
fn absent_casefold_aliases_are_rejected_without_side_effects() {
    let temp = tempfile::tempdir().unwrap();
    let paths = vec![temp.path().join("Update.JSON"), temp.path().join("update.json")];

    assert!(matches!(
        validate_distinct_paths(&paths),
        Err(UpdateError::InvalidLayout { .. })
    ));
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
}

#[test]
fn existing_inode_aliases_are_rejected_without_new_side_effects() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    std::fs::write(&first, b"sentinel").unwrap();
    std::fs::hard_link(&first, &second).unwrap();
    let before = std::fs::read_dir(temp.path()).unwrap().count();

    assert!(matches!(
        validate_distinct_paths(&[first, second]),
        Err(UpdateError::InvalidLayout { .. })
    ));
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), before);
}

#[test]
fn symlink_aliases_are_rejected_without_new_side_effects() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    std::fs::write(&first, b"sentinel").unwrap();
    symlink(&first, &second).unwrap();
    let before = std::fs::read_dir(temp.path()).unwrap().count();

    assert!(matches!(
        validate_distinct_paths(&[first, second]),
        Err(UpdateError::InvalidLayout { .. })
    ));
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), before);
}
