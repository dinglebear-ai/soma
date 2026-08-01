use super::*;

#[test]
fn errors_preserve_domain_and_target_identity() {
    let error = InfraError::CommandFailed {
        domain: "host",
        host: HostId::new("dookie").unwrap(),
        exit_code: Some(1),
        stderr: "nope".into(),
    };
    assert_eq!(
        error.to_string(),
        "host command failed on dookie with exit Some(1): nope"
    );
}

#[test]
fn filesystem_errors_do_not_hide_requested_path() {
    let error = InfraError::PathOutsideRoots("/etc/shadow".into());
    assert!(error.to_string().contains("/etc/shadow"));
}
