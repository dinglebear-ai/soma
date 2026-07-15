//! SSRF preflight guard for CIMD `client_id` URL fetches.
//!
//! This is a *static* preflight — it does not perform DNS resolution. It
//! rejects non-https schemes, userinfo, query/fragment components, a
//! missing or root-only path, private-TLD-suffixed hostnames, textual
//! loopback hostnames, and IP-literal hosts that fall in a private/
//! loopback/link-local/CGNAT/ULA/transition-mechanism/multicast range.
//! Callers that resolve a domain name to an IP address MUST additionally
//! run each resolved address through [`check_ip_not_private`] before
//! connecting, and MUST re-validate the actual TCP peer post-connect (see
//! `cimd::document::resolve_and_validate_address` and
//! `cimd::document::fetch_document_at`) — this module alone does not close
//! the DNS-rebinding/proxy-interception gap by itself.
//!
//! Adapted (not imported — this crate has no path dependency on the
//! sibling `lab` repo) from the equivalent guard in
//! `labby-primitives::ssrf` and its caller,
//! `labby-apis::acp_registry::installer`, which additionally documents and
//! implements post-connect peer re-validation as "the load-bearing line of
//! the SSRF TOCTOU / DNS-rebinding defense" — that additional layer is
//! implemented in `cimd::document`, not here (this module only covers the
//! static/pre-DNS portion of the reference's rigor).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Reason a CIMD `client_id` URL was rejected.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SsrfError {
    /// URL could not be parsed, used a non-https scheme, lacked a host,
    /// carried forbidden userinfo/query/fragment, or lacked a path
    /// component.
    #[error("{0}")]
    InvalidUrl(String),
    /// Host resolved (or parsed) to a private/loopback/link-local/CGNAT/
    /// ULA/transition-mechanism/multicast address, or matched a
    /// private-TLD suffix denylist.
    #[error("{0}")]
    Blocked(String),
}

impl SsrfError {
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidUrl(_) => "invalid_param",
            Self::Blocked(_) => "ssrf_blocked",
        }
    }
}

/// Private-DNS suffix denylist applied to non-IP hosts. Belt-and-suspenders
/// on top of the resolved-IP checks (an internal name might resolve through
/// split-horizon DNS to a public-looking record at validation time).
pub const PRIVATE_TLD_SUFFIXES: &[&str] =
    &[".local", ".internal", ".lan", ".intranet", ".corp", ".home"];

/// Returns `true` for the IPv4 carrier-grade NAT range `100.64.0.0/10`.
#[must_use]
pub fn is_cgnat(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 100 && (64..=127).contains(&octets[1])
}

/// Returns `true` for `0.0.0.0/8` ("this network", broader than the single
/// unspecified address), IPv4 multicast (`224.0.0.0/4`), Class E reserved
/// space (`240.0.0.0/4`, RFC 1112 §4 — `Ipv4Addr::is_reserved` covers this
/// but is unstable, so the range check is inlined here), and the limited
/// broadcast address `255.255.255.255`.
#[must_use]
pub fn is_ipv4_reserved_broadcast_or_multicast(ip: Ipv4Addr) -> bool {
    ip.octets()[0] == 0 || ip.is_multicast() || ip.is_broadcast() || (ip.octets()[0] & 0xf0) == 0xf0
}

fn is_ipv6_link_local(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

fn is_ipv6_ula(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

/// Returns `true` for the deprecated IPv4-compatible IPv6 form (`::a.b.c.d`,
/// distinct from the IPv4-*mapped* `::ffff:a.b.c.d` form already handled
/// separately via `to_ipv4_mapped()`). Some stacks still route this form;
/// treat any non-loopback, non-unspecified address here as embedding a
/// target IPv4 address this guard has not yet validated.
#[must_use]
pub fn is_ipv6_ipv4_compatible(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    s[0] == 0
        && s[1] == 0
        && s[2] == 0
        && s[3] == 0
        && s[4] == 0
        && s[5] == 0
        && !ip.is_loopback()
        && !ip.is_unspecified()
}

/// Returns `true` for IPv6 transition-mechanism prefixes that embed an
/// IPv4 address reachable through the mechanism — NAT64 (`64:ff9b::/96`),
/// 6to4 (`2002::/16`), and Teredo (`2001::/32`). These are more credible
/// SSRF bypasses than IPv6 documentation ranges and are blocked wholesale
/// (not selectively unwrapped) since this is a conservative preflight for
/// an OAuth Authorization Server fetching an attacker-supplied URL, not a
/// general-purpose network client that needs transition-mechanism support.
#[must_use]
pub fn is_ipv6_transition_mechanism(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    (s[0] == 0x0064 && s[1] == 0xff9b && s[2] == 0 && s[3] == 0 && s[4] == 0 && s[5] == 0)
        || s[0] == 0x2002
        || (s[0] == 0x2001 && s[1] == 0)
}

/// Reject an IP that targets private, loopback, link-local, CGNAT, ULA,
/// IPv4-mapped-private, IPv4-compatible, transition-mechanism, reserved,
/// multicast, or broadcast space. `context` is a non-secret label (redacted
/// URL or bare host) used only to build the error message.
///
/// # Errors
/// Returns [`SsrfError::Blocked`] when `ip` falls in any blocked range.
pub fn check_ip_not_private(ip: IpAddr, context: &str) -> Result<(), SsrfError> {
    let normalized = match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(v6),
        },
        other => other,
    };

    let blocked = match normalized {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || is_cgnat(v4)
                || is_ipv4_reserved_broadcast_or_multicast(v4)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || is_ipv6_link_local(v6)
                || is_ipv6_ula(v6)
                || is_ipv6_ipv4_compatible(v6)
                || is_ipv6_transition_mechanism(v6)
        }
    };

    if blocked {
        return Err(SsrfError::Blocked(format!(
            "`{context}` resolves to a private, loopback, link-local, CGNAT, ULA, transition-mechanism, reserved, multicast, or broadcast address {ip}; blocked to prevent SSRF"
        )));
    }

    Ok(())
}

fn check_host_not_private(host: &str) -> Result<(), SsrfError> {
    let host_lower = host.to_ascii_lowercase();
    let host_lower = host_lower.strip_suffix('.').unwrap_or(&host_lower);
    if host_lower == "localhost"
        || host_lower.starts_with("127.")
        || host_lower == "::1"
        || host_lower.contains("::ffff:")
        || host_lower == "0.0.0.0"
        || PRIVATE_TLD_SUFFIXES.iter().any(|s| host_lower.ends_with(s))
    {
        return Err(SsrfError::Blocked(format!(
            "host `{host}` is a local/loopback/private address"
        )));
    }
    Ok(())
}

fn redact_url(raw: &str) -> String {
    match url::Url::parse(raw) {
        Ok(mut url) => {
            // `set_username`/`set_password` return `Err(())` for URLs that
            // "cannot be a base" (e.g. non-hierarchical schemes). This
            // function exists solely to produce a safe-to-log string, so a
            // redaction step that can't be confirmed to have removed
            // userinfo must not silently emit the original unredacted URL.
            if url.set_username("").is_err() || url.set_password(None).is_err() {
                return "<url-with-unredactable-userinfo>".to_string();
            }
            url.set_query(None);
            url.set_fragment(None);
            url.to_string()
        }
        Err(_) => "<invalid-url>".to_string(),
    }
}

/// Parse and statically validate a CIMD `client_id` URL: require https,
/// forbid userinfo/query/fragment, require a non-root path component,
/// require a host, and reject the private-TLD/loopback host denylist. If
/// the host is an IP literal it is additionally run through
/// [`check_ip_not_private`].
///
/// This performs **no DNS** — see the module doc for what callers must do
/// after resolving a domain-name host.
///
/// # Errors
/// Returns [`SsrfError`] when any static rule is violated.
pub fn validate_url_shape(url: &str) -> Result<url::Url, SsrfError> {
    let redacted = redact_url(url);
    let parsed = url::Url::parse(url)
        .map_err(|e| SsrfError::InvalidUrl(format!("invalid URL `{redacted}`: {e}")))?;

    if parsed.scheme() != "https" {
        return Err(SsrfError::InvalidUrl(format!(
            "URL `{redacted}` must use https to prevent SSRF"
        )));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(SsrfError::InvalidUrl(format!(
            "URL `{redacted}` must not include userinfo"
        )));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(SsrfError::InvalidUrl(format!(
            "URL `{redacted}` must not include query or fragment components"
        )));
    }
    if parsed.path().is_empty() || parsed.path() == "/" {
        return Err(SsrfError::InvalidUrl(format!(
            "URL `{redacted}` must contain a path component (see docs/references/mcp/client-id-metadata-document.md)"
        )));
    }

    match parsed.host() {
        Some(url::Host::Domain(domain)) => check_host_not_private(domain)?,
        Some(url::Host::Ipv4(ip)) => check_ip_not_private(IpAddr::V4(ip), &redacted)?,
        Some(url::Host::Ipv6(ip)) => check_ip_not_private(IpAddr::V6(ip), &redacted)?,
        None => {
            return Err(SsrfError::InvalidUrl(format!(
                "URL `{redacted}` must include a host"
            )));
        }
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_private_ranges_exactly() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.1.1",
            "169.254.169.254", // cloud metadata service
            "100.64.0.1",
            "100.127.255.255",
            "0.5.5.5",         // 0.0.0.0/8 "this network"
            "224.0.0.1",       // multicast
            "240.0.0.1",       // Class E reserved
            "255.255.255.254", // Class E reserved (top of range, below broadcast)
            "255.255.255.255", // limited broadcast
            "::1",
            "fe80::1",
            "fc00::1",
            "fd00::1",
            "ff02::1", // IPv6 multicast
            "::ffff:127.0.0.1",
            "::ffff:10.1.2.3",
            "::ffff:100.64.0.1",
            "::ffff:169.254.169.254",
            "::7f00:1",        // IPv4-compatible IPv6 form of 127.0.0.1
            "64:ff9b::7f00:1", // NAT64-embedded loopback
            "2002::1",         // 6to4
            "2001::1",         // Teredo
        ] {
            let parsed: IpAddr = ip.parse().expect(ip);
            let err = check_ip_not_private(parsed, "app.example.com").unwrap_err();
            assert_eq!(err.kind(), "ssrf_blocked", "{ip}");
        }
    }

    #[test]
    fn allows_public_addresses() {
        for ip in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            let parsed: IpAddr = ip.parse().expect(ip);
            check_ip_not_private(parsed, "app.example.com").expect(ip);
        }
    }

    #[test]
    fn rejects_non_https_as_invalid_param() {
        let err =
            validate_url_shape("http://app.example.com/oauth/client-metadata.json").unwrap_err();
        assert_eq!(err.kind(), "invalid_param");
    }

    #[test]
    fn rejects_missing_path_as_invalid_param() {
        let err = validate_url_shape("https://app.example.com").unwrap_err();
        assert_eq!(err.kind(), "invalid_param");
        let err_root = validate_url_shape("https://app.example.com/").unwrap_err();
        assert_eq!(err_root.kind(), "invalid_param");
    }

    #[test]
    fn rejects_userinfo() {
        let err = validate_url_shape("https://user@app.example.com/client.json").unwrap_err();
        assert_eq!(err.kind(), "invalid_param");
    }

    #[test]
    fn rejects_query_and_fragment() {
        let err_query = validate_url_shape("https://app.example.com/client.json?x=1").unwrap_err();
        assert_eq!(err_query.kind(), "invalid_param");
        let err_fragment = validate_url_shape("https://app.example.com/client.json#x").unwrap_err();
        assert_eq!(err_fragment.kind(), "invalid_param");
    }

    #[test]
    fn rejects_private_and_loopback_hosts_as_blocked() {
        for url in [
            "https://app.local/client.json",
            "https://127.0.0.1/client.json",
            "https://[::ffff:127.0.0.1]/client.json",
            "https://192.168.1.20/client.json",
        ] {
            let err = validate_url_shape(url).unwrap_err();
            assert_eq!(err.kind(), "ssrf_blocked", "{url}");
        }
    }

    #[test]
    fn rejects_bracketed_ipv6_literals() {
        for url in [
            "https://[::1]/client.json",
            "https://[fe80::1]/client.json",
            "https://[fc00::1]/client.json",
        ] {
            let err = validate_url_shape(url).unwrap_err();
            assert_eq!(err.kind(), "ssrf_blocked", "{url}");
        }
    }

    #[test]
    fn private_tld_suffixes_are_blocked() {
        for host_url in [
            "https://box.local/c.json",
            "https://svc.internal/c.json",
            "https://host.lan/c.json",
        ] {
            let err = validate_url_shape(host_url).unwrap_err();
            assert_eq!(err.kind(), "ssrf_blocked", "{host_url}");
        }
    }

    #[test]
    fn private_tld_suffix_bypass_via_trailing_dot_is_blocked() {
        // "svc.internal." (FQDN trailing dot) resolves identically to
        // "svc.internal" and must not bypass the suffix denylist.
        let err = validate_url_shape("https://svc.internal./c.json").unwrap_err();
        assert_eq!(err.kind(), "ssrf_blocked");
    }

    #[test]
    fn allows_valid_public_https_url_with_path() {
        let parsed = validate_url_shape("https://app.example.com/oauth/client-metadata.json")
            .expect("should validate");
        assert_eq!(parsed.host_str(), Some("app.example.com"));
    }
}
