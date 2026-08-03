use super::*;

fn fingerprint(path: &str) -> BuildContextFingerprint {
    BuildContextFingerprint {
        host: soma_fleet::HostId::new("dookie").unwrap(),
        topology_revision: soma_fleet::TopologyRevision::from_material(b"test"),
        path: path.into(),
        sha256: "a".repeat(64),
        file_count: 1,
        byte_count: 1,
    }
}

#[test]
fn image_build_requests_reject_context_and_dockerfile_escape() {
    let op = OperationName::new("docker.build").unwrap();
    let deadline = Timestamp::now();
    assert!(
        ImageBuildRequest::new(
            OperationId::new(),
            op.clone(),
            "relative".into(),
            None,
            "app:v1",
            false,
            fingerprint("relative"),
            deadline
        )
        .is_err()
    );
    assert!(
        ImageBuildRequest::new(
            OperationId::new(),
            op.clone(),
            "/srv/app".into(),
            Some("../Dockerfile".into()),
            "app:v1",
            false,
            fingerprint("/srv/app"),
            deadline
        )
        .is_err()
    );
    assert!(
        ImageBuildRequest::new(
            OperationId::new(),
            op,
            "/srv/app".into(),
            None,
            "--tag",
            false,
            fingerprint("/srv/app"),
            deadline
        )
        .is_err()
    );
}
