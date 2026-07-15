use super::*;

#[test]
fn strict_external_denies_loopback_private_cgnat_and_metadata_targets() {
    for url in [
        "https://127.0.0.1/mcp",
        "https://10.0.0.2/mcp",
        "https://100.64.1.1/mcp",
        "https://169.254.169.254/latest",
        "https://printer.local/mcp",
    ] {
        assert!(
            validate_url(url, OutboundPolicy::StrictExternal).is_err(),
            "{url}"
        );
    }
}

#[test]
fn admin_backend_allows_lan_but_not_localhost_or_metadata() {
    validate_url("http://10.0.0.2/mcp", OutboundPolicy::AdminProtectedBackend).unwrap();
    assert!(validate_url(
        "http://127.0.0.1/mcp",
        OutboundPolicy::AdminProtectedBackend
    )
    .is_err());
    assert!(validate_url(
        "http://169.254.169.254/latest",
        OutboundPolicy::AdminProtectedBackend
    )
    .is_err());
}

#[test]
fn strict_external_requires_https_and_denies_userinfo() {
    assert_eq!(
        validate_url("http://example.com/mcp", OutboundPolicy::StrictExternal).unwrap_err(),
        SsrfError::InvalidScheme
    );
    assert_eq!(
        validate_url(
            "https://user:pass@example.com/mcp",
            OutboundPolicy::StrictExternal
        )
        .unwrap_err(),
        SsrfError::UserInfoDenied
    );
}

#[test]
fn redirects_are_rechecked_under_same_policy() {
    let endpoint = validate_url("https://example.com/mcp", OutboundPolicy::StrictExternal).unwrap();
    assert!(validate_redirect(&endpoint, "https://127.0.0.1/mcp").is_err());
}
