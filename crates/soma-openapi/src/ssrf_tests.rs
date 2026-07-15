use std::net::IpAddr;

use crate::config::{OpenApiSpecConfig, SpecSource};
use crate::ssrf::{
    check_host_not_private, check_ip_not_private, parse_validated_https_url, redact_url,
    validate_base_url, validate_spec_url,
};

fn spec(base: &str) -> OpenApiSpecConfig {
    OpenApiSpecConfig {
        label: "vendor".into(),
        spec_source: SpecSource::Url("https://api.example.com/openapi.json".parse().unwrap()),
        base_url: base.parse().unwrap(),
        allowed_operations: vec![],
        credential: None,
    }
}

#[test]
fn public_https_ok() {
    assert!(validate_base_url(&spec("https://api.example.com")).is_ok());
}

#[test]
fn rejects_private_ranges_exactly() {
    for ip in [
        "127.0.0.1",
        "10.1.2.3",
        "172.16.0.1",
        "192.168.1.1",
        "169.254.1.1",
        "100.64.0.1",
        "100.127.255.255",
        "::1",
        "fe80::1",
        "fc00::1",
        "fd00::1",
        "::ffff:127.0.0.1",
        "::ffff:10.1.2.3",
        "::ffff:100.64.0.1",
        "::ffff:169.254.169.254",
    ] {
        let parsed: IpAddr = ip.parse().expect(ip);
        let err = check_ip_not_private(parsed, "registry.example.com").unwrap_err();
        assert_eq!(err.kind(), "ssrf_blocked", "{ip}");
    }
}

#[test]
fn rejects_ipv4_class_e_and_ipv6_multicast_as_soma_hardening() {
    for ip in ["240.0.0.1", "255.255.255.255", "ff02::1"] {
        let parsed: IpAddr = ip.parse().expect(ip);
        let err = check_ip_not_private(parsed, "registry.example.com").unwrap_err();
        assert_eq!(err.kind(), "ssrf_blocked", "{ip}");
    }
}

#[test]
fn allows_public_addresses() {
    for ip in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
        let parsed: IpAddr = ip.parse().expect(ip);
        check_ip_not_private(parsed, "cdn.example.com").expect(ip);
    }
}

#[test]
fn rejects_non_https_as_invalid_param() {
    let err = parse_validated_https_url("http://example.com/agent.tar.gz").unwrap_err();
    assert_eq!(err.kind(), "invalid_param");
}

#[test]
fn rejects_private_and_loopback_hosts_as_blocked() {
    for url in [
        "https://agent.local/agent.tar.gz",
        "https://127.0.0.1/agent.tar.gz",
        "https://[::ffff:127.0.0.1]/agent.tar.gz",
        "https://192.168.1.20/agent.tar.gz",
    ] {
        let err = parse_validated_https_url(url).unwrap_err();
        assert_eq!(err.kind(), "ssrf_blocked", "{url}");
    }
}

#[test]
fn rejects_bracketed_ipv6_literals_through_full_url_path() {
    for url in [
        "https://[::1]/agent.tar.gz",
        "https://[fe80::1]/agent.tar.gz",
        "https://[fc00::1]/agent.tar.gz",
        "https://[fd00::1]/agent.tar.gz",
    ] {
        let err = parse_validated_https_url(url).unwrap_err();
        assert_eq!(err.kind(), "ssrf_blocked", "{url}");
    }
}

#[test]
fn rejects_userinfo_query_and_fragment_without_leaking_secret() {
    for url in [
        "https://user@example.com/a.tar.gz",
        "https://example.com/a.tar.gz?token=secret",
        "https://example.com/a.tar.gz#secret",
    ] {
        let err = parse_validated_https_url(url).unwrap_err();
        assert_eq!(err.kind(), "invalid_param");
        assert!(!err.to_string().contains("secret"), "{url}");
    }
}

#[test]
fn private_tld_suffixes_are_blocked() {
    for host in [
        "box.local",
        "svc.internal",
        "host.lan",
        "x.intranet",
        "y.corp",
        "z.home",
    ] {
        let err = check_host_not_private(host).unwrap_err();
        assert_eq!(err.kind(), "ssrf_blocked", "{host}");
    }
}

#[test]
fn spec_url_public_https_ok() {
    let url = "https://api.example.com/openapi.json".parse().unwrap();
    assert!(validate_spec_url("vendor", &url).is_ok());
}

#[test]
fn spec_url_private_and_non_https_rejected() {
    let rfc1918 = "https://10.0.0.5/openapi.json".parse().unwrap();
    assert!(validate_spec_url("vendor", &rfc1918).is_err());
    let http = "http://api.example.com/openapi.json".parse().unwrap();
    assert!(validate_spec_url("vendor", &http).is_err());
    let loopback = "https://127.0.0.1/openapi.json".parse().unwrap();
    assert!(validate_spec_url("vendor", &loopback).is_err());
}

#[test]
fn redact_url_strips_userinfo_query_and_fragment() {
    let redacted = redact_url("https://user:pass@example.com/a?token=secret#frag");
    assert_eq!(redacted, "https://example.com/a");
}
