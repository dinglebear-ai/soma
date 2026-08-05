use super::*;

#[test]
fn build_context_policy_requires_explicit_bounded_roots() {
    assert!(BuildContextPolicy::new(Vec::<PathBuf>::new()).is_err());
    assert!(BuildContextPolicy::new(["relative"]).is_err());
    let policy = BuildContextPolicy::new(["/srv/builds", "/opt/src"])
        .unwrap()
        .with_limits(10, 4096)
        .unwrap();
    assert_eq!(policy.max_files(), 10);
    assert_eq!(policy.max_bytes(), 4096);
    assert!(policy.resolve(Path::new("/srv/builds/app")).is_ok());
    assert!(policy.resolve(Path::new("/etc")).is_err());
}

#[test]
fn fingerprints_require_lowercase_sha256() {
    let host = soma_fleet::HostId::new("devhost").unwrap();
    let fingerprint = BuildContextFingerprint {
        host,
        topology_revision: soma_fleet::TopologyRevision::from_material(b"test"),
        path: "/srv/builds/app".into(),
        sha256: "a".repeat(64),
        file_count: 1,
        byte_count: 10,
    };
    fingerprint.validate().unwrap();
    let mut invalid = fingerprint;
    invalid.sha256 = "ABC".into();
    assert!(invalid.validate().is_err());
}
