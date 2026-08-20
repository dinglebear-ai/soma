use super::*;

#[test]
fn canonical_worktree_paths_must_be_absolute_without_parent_segments() {
    assert!(canonical_absolute("/workspace/soma"));
    assert!(canonical_absolute("/workspace/./soma"));
    assert!(!canonical_absolute("workspace/soma"));
    assert!(!canonical_absolute("/workspace/../soma"));
}
