use std::net::IpAddr;

use thiserror::Error;
use url::{Host, Url};

use crate::net;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboundPolicy {
    StrictExternal,
    AdminProtectedBackend,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedEndpoint {
    pub redacted_url: String,
    pub host: String,
    pub policy: OutboundPolicy,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SsrfError {
    #[error("invalid URL")]
    InvalidUrl,
    #[error("URL scheme is not allowed")]
    InvalidScheme,
    #[error("URL must not include credentials")]
    UserInfoDenied,
    #[error("host is denied by outbound policy")]
    HostDenied,
    #[error("IP address is denied by outbound policy")]
    IpDenied,
    #[error("redirect target is denied by outbound policy")]
    RedirectDenied,
}

pub fn validate_url(raw: &str, policy: OutboundPolicy) -> Result<ValidatedEndpoint, SsrfError> {
    let parsed = Url::parse(raw.trim()).map_err(|_| SsrfError::InvalidUrl)?;
    validate_parsed_url(&parsed, policy)?;
    let host = parsed.host_str().ok_or(SsrfError::HostDenied)?.to_owned();
    Ok(ValidatedEndpoint {
        redacted_url: crate::security::redact::redact_url(raw),
        host,
        policy,
    })
}

pub fn validate_redirect(
    original: &ValidatedEndpoint,
    redirect: &str,
) -> Result<ValidatedEndpoint, SsrfError> {
    validate_url(redirect, original.policy).map_err(|_| SsrfError::RedirectDenied)
}

pub fn validate_resolved_ip(ip: IpAddr, policy: OutboundPolicy) -> Result<(), SsrfError> {
    if net::is_denied_loopback_or_wildcard(ip)
        || net::is_link_local(ip)
        || net::is_metadata_ip(ip)
        || policy == OutboundPolicy::StrictExternal && net::is_private_or_cgnat(ip)
    {
        return Err(SsrfError::IpDenied);
    }
    Ok(())
}

fn validate_parsed_url(parsed: &Url, policy: OutboundPolicy) -> Result<(), SsrfError> {
    match parsed.scheme() {
        "https" => {}
        "http" if policy == OutboundPolicy::AdminProtectedBackend => {}
        _ => return Err(SsrfError::InvalidScheme),
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(SsrfError::UserInfoDenied);
    }
    let host = parsed.host().ok_or(SsrfError::HostDenied)?;
    match host {
        Host::Ipv4(ip) => validate_resolved_ip(IpAddr::V4(ip), policy)?,
        Host::Ipv6(ip) => validate_resolved_ip(IpAddr::V6(ip), policy)?,
        Host::Domain(host) if net::host_is_private_tld_surprise(host) => {
            return Err(SsrfError::HostDenied);
        }
        Host::Domain(_) => {}
    }
    Ok(())
}

#[cfg(test)]
#[path = "ssrf_tests.rs"]
mod tests;
