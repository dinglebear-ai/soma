use super::*;

#[test]
fn policies_require_absolute_roots_and_closed_limits() {
    assert!(FileReadPolicy::new(Vec::<PathBuf>::new()).is_err());
    assert!(FileReadPolicy::new(["relative"]).is_err());
    assert!(FileReadPolicy::new(["/srv/../etc"]).is_err());
    let policy = FileReadPolicy::new(["/srv", "/srv/data", "/srv"]).unwrap();
    assert_eq!(
        policy.roots().collect::<Vec<_>>(),
        vec![Path::new("/srv"), Path::new("/srv/data")]
    );
    assert!(policy.clone().with_preview_limit(0).is_err());
    assert!(
        policy
            .clone()
            .with_preview_limit(16 * 1024 * 1024 + 1)
            .is_err()
    );
    assert!(policy.clone().with_hash_limit(0).is_err());
}

#[test]
fn longest_matching_root_wins_and_outside_paths_fail() {
    let policy = FileReadPolicy::new(["/srv", "/srv/data"]).unwrap();
    let (root, relative) = policy.resolve(Path::new("/srv/data/file.txt")).unwrap();
    assert_eq!(root, PathBuf::from("/srv/data"));
    assert_eq!(relative, PathBuf::from("file.txt"));
    assert!(matches!(
        policy.resolve(Path::new("/etc/passwd")),
        Err(InfraError::PathOutsideRoots(_))
    ));
    assert!(policy.resolve(Path::new("/srv/../etc/passwd")).is_err());
}
