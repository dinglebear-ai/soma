use super::*;
use std::net::{IpAddr, Ipv4Addr};

#[test]
fn classifies_loopback_link_local_private_and_metadata_ips() {
    assert!(is_denied_loopback_or_wildcard(IpAddr::V4(
        Ipv4Addr::LOCALHOST
    )));
    assert!(is_link_local(IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1))));
    assert!(is_private_or_cgnat(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
    assert!(is_private_or_cgnat(IpAddr::V4(Ipv4Addr::new(
        100, 64, 1, 1
    ))));
    assert!(is_metadata_ip(IpAddr::V4(Ipv4Addr::new(
        169, 254, 169, 254
    ))));
}

#[test]
fn private_tld_surprises_include_bare_and_local_hosts() {
    assert!(host_is_private_tld_surprise("localhost"));
    assert!(host_is_private_tld_surprise("printer.local"));
    assert!(host_is_private_tld_surprise("nas"));
    assert!(!host_is_private_tld_surprise("example.com"));
}
