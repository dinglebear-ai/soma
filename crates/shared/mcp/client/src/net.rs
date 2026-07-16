use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[must_use]
pub fn is_denied_loopback_or_wildcard(ip: IpAddr) -> bool {
    ip.is_loopback() || ip.is_unspecified()
}

#[must_use]
pub fn is_link_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_link_local(),
        IpAddr::V6(ip) => ip.is_unicast_link_local(),
    }
}

#[must_use]
pub fn is_private_or_cgnat(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private() || ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1])
        }
        IpAddr::V6(ip) => ip.is_unique_local(),
    }
}

#[must_use]
pub fn is_metadata_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip == Ipv4Addr::new(169, 254, 169, 254),
        IpAddr::V6(ip) => ip == Ipv6Addr::LOCALHOST,
    }
}

#[must_use]
pub fn host_is_private_tld_surprise(host: &str) -> bool {
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    normalized == "localhost"
        || !normalized.contains('.')
        || normalized.ends_with(".local")
        || normalized.ends_with(".internal")
        || normalized.ends_with(".lan")
}

#[cfg(test)]
#[path = "net_tests.rs"]
mod tests;
