use std::net::{IpAddr, SocketAddr};

use crate::error::OpenApiError;

#[test]
fn validates_every_dns_answer_before_pinning() {
    let addrs = vec![
        SocketAddr::from(([1, 1, 1, 1], 443)),
        SocketAddr::from(([10, 0, 0, 1], 443)),
    ];
    let err =
        super::resolve::validate_resolved_addrs("api.example.com", "vendor", addrs).unwrap_err();
    assert_eq!(err.kind(), "forbidden");
}

#[test]
fn empty_resolution_is_resolve_failed() {
    let err =
        super::resolve::validate_resolved_addrs("api.example.com", "vendor", vec![]).unwrap_err();
    assert_eq!(err.kind(), "internal_error");
    assert!(matches!(err, OpenApiError::ResolveFailed { .. }));
}

#[test]
fn remote_addr_missing_fails_closed() {
    let pinned: IpAddr = "1.1.1.1".parse().unwrap();
    let err = super::resolve::validate_remote_addr(None, pinned, "vendor").unwrap_err();
    assert_eq!(err.kind(), "forbidden");
}

#[test]
fn remote_addr_mismatch_fails_closed() {
    let pinned: IpAddr = "1.1.1.1".parse().unwrap();
    let remote = SocketAddr::from(([8, 8, 8, 8], 443));
    let err = super::resolve::validate_remote_addr(Some(remote), pinned, "vendor").unwrap_err();
    assert_eq!(err.kind(), "forbidden");
}

#[test]
fn remote_addr_private_fails_even_when_pinned() {
    let pinned: IpAddr = "10.0.0.1".parse().unwrap();
    let remote = SocketAddr::from(([10, 0, 0, 1], 443));
    let err = super::resolve::validate_remote_addr(Some(remote), pinned, "vendor").unwrap_err();
    assert_eq!(err.kind(), "forbidden");
}

#[tokio::test]
async fn ipv6_literal_host_resolves_then_is_ssrf_checked() {
    let err = super::resolve::resolve_and_validate("[::1]", 443, "vendor")
        .await
        .expect_err("loopback must be rejected");
    assert_eq!(err.kind(), "forbidden");
}
