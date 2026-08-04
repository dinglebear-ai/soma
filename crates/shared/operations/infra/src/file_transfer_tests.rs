use super::*;

#[test]
fn transfer_policy_requires_named_files_beneath_roots() {
    let policy = FileTransferPolicy::new(["/srv/source"], ["/srv/destination"]).unwrap();
    assert!(
        policy
            .resolve_source(Path::new("/srv/source/file.txt"))
            .is_ok()
    );
    assert!(policy.resolve_source(Path::new("/srv/source")).is_err());
    assert!(
        policy
            .resolve_destination(Path::new("/tmp/file.txt"))
            .is_err()
    );
}

#[test]
fn byte_identity_is_deterministic() {
    let left = identity_from_bytes(Path::new("/tmp/a"), b"hello");
    let right = identity_from_bytes(Path::new("/tmp/a"), b"hello");
    assert_eq!(left, right);
    assert_eq!(left.sha256.len(), 64);
}
