use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::config::OpenApiSpecConfig;
use crate::error::{OpenApiError, SsrfError};

pub const PRIVATE_TLD_SUFFIXES: &[&str] =
    &[".local", ".internal", ".lan", ".intranet", ".corp", ".home"];

#[must_use]
pub fn is_cgnat(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 100 && (64..=127).contains(&octets[1])
}

fn is_ipv6_link_local(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

fn is_ipv6_ula(ip: Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

fn is_ipv4_class_e(ip: Ipv4Addr) -> bool {
    ip.octets()[0] >= 240
}

pub fn check_ip_not_private(ip: IpAddr, context: &str) -> Result<(), SsrfError> {
    let normalized = match ip {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(v6)),
        other => other,
    };

    let blocked = match normalized {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || is_cgnat(v4)
                // Intentional Soma hardening beyond the current Lab policy.
                || is_ipv4_class_e(v4)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || is_ipv6_link_local(v6)
                || is_ipv6_ula(v6)
                // Intentional Soma hardening beyond the current Lab policy.
                || v6.is_multicast()
        }
    };

    if blocked {
        return Err(SsrfError::Blocked(format!(
            "`{context}` resolves to a private, loopback, link-local, CGNAT, ULA, Class E, or multicast address {ip}; blocked to prevent SSRF"
        )));
    }

    Ok(())
}

pub fn check_host_not_private(host: &str) -> Result<(), SsrfError> {
    let host_lower = host.to_ascii_lowercase();
    if host_lower == "localhost"
        || host_lower.starts_with("127.")
        || host_lower == "::1"
        || host_lower.contains("::ffff:")
        || host_lower == "0.0.0.0"
        || PRIVATE_TLD_SUFFIXES
            .iter()
            .any(|suffix| host_lower.ends_with(suffix))
    {
        return Err(SsrfError::Blocked(format!(
            "host `{host}` is a local/loopback/private address"
        )));
    }
    Ok(())
}

#[must_use]
pub fn redact_url(raw: &str) -> String {
    match url::Url::parse(raw) {
        Ok(mut url) => {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.set_query(None);
            url.set_fragment(None);
            url.to_string()
        }
        Err(_) => "<invalid-url>".to_string(),
    }
}

pub fn parse_validated_https_url(url: &str) -> Result<url::Url, SsrfError> {
    let redacted = redact_url(url);
    let parsed = url::Url::parse(url)
        .map_err(|error| SsrfError::InvalidUrl(format!("invalid URL `{redacted}`: {error}")))?;

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

pub fn validate_base_url(cfg: &OpenApiSpecConfig) -> Result<url::Url, OpenApiError> {
    validate_https_url(&cfg.label, &cfg.base_url)?;
    Ok(cfg.base_url.clone())
}

pub fn validate_spec_url(label: &str, url: &url::Url) -> Result<(), OpenApiError> {
    validate_https_url(label, url)
}

fn validate_https_url(label: &str, url: &url::Url) -> Result<(), OpenApiError> {
    parse_validated_https_url(url.as_str())
        .map(|_| ())
        .map_err(|error| OpenApiError::SsrfRejected {
            label: label.to_string(),
            reason: error.kind().to_string(),
        })
}
