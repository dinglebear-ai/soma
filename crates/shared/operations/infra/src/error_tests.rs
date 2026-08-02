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

#[test]
fn public_diagnostics_are_small_and_strip_controls() {
    let mut raw = b"failure\n\x1b[31mred\x1b[0m\0".to_vec();
    raw.extend(std::iter::repeat_n(b'x', 4096));
    let diagnostic = public_diagnostic(&raw);
    assert!(!diagnostic.contains('\u{1b}'));
    assert!(!diagnostic.contains('\n'));
    assert!(diagnostic.len() <= PUBLIC_DIAGNOSTIC_LIMIT);
    assert!(diagnostic.starts_with("failure red"));
}
