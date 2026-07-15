use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use crate::error::OpenApiError;

const RESOLVE_TIMEOUT: Duration = Duration::from_secs(3);

pub(crate) async fn pinned_client_for(
    url: &url::Url,
    label: &str,
) -> Result<(reqwest::Client, IpAddr), OpenApiError> {
    let host = url.host_str().ok_or_else(|| OpenApiError::ResolveFailed {
        label: label.to_string(),
    })?;
    let port = url.port_or_known_default().unwrap_or(443);
    let addrs = resolve_and_validate(host, port, label).await?;
    let pinned = addrs[0];
    let client = super::client::base_builder()
        .resolve_to_addrs(host, &[pinned])
        .build()
        .map_err(|_| OpenApiError::ClientBuildFailed)?;
    Ok((client, pinned.ip()))
}

pub(crate) async fn resolve_and_validate(
    host: &str,
    port: u16,
    label: &str,
) -> Result<Vec<SocketAddr>, OpenApiError> {
    let lookup_host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    let addrs: Vec<_> = tokio::time::timeout(
        RESOLVE_TIMEOUT,
        tokio::net::lookup_host((lookup_host, port)),
    )
    .await
    .map_err(|_| OpenApiError::ResolveFailed {
        label: label.to_string(),
    })?
    .map_err(|_| OpenApiError::ResolveFailed {
        label: label.to_string(),
    })?
    .collect();
    validate_resolved_addrs(host, label, addrs)
}

pub(crate) fn validate_resolved_addrs(
    host: &str,
    label: &str,
    addrs: Vec<SocketAddr>,
) -> Result<Vec<SocketAddr>, OpenApiError> {
    if addrs.is_empty() {
        return Err(OpenApiError::ResolveFailed {
            label: label.to_string(),
        });
    }
    for addr in &addrs {
        crate::ssrf::check_ip_not_private(addr.ip(), host).map_err(|_| {
            OpenApiError::RequestBlockedPrivateAddr {
                label: label.to_string(),
            }
        })?;
    }
    Ok(addrs)
}

pub(crate) fn recheck_peer(
    response: &reqwest::Response,
    pinned: IpAddr,
    label: &str,
) -> Result<(), OpenApiError> {
    validate_remote_addr(response.remote_addr(), pinned, label)
}

pub(crate) fn validate_remote_addr(
    remote: Option<SocketAddr>,
    pinned: IpAddr,
    label: &str,
) -> Result<(), OpenApiError> {
    let peer = remote.ok_or_else(|| OpenApiError::RequestBlockedPrivateAddr {
        label: label.to_string(),
    })?;
    if peer.ip() != pinned {
        return Err(OpenApiError::RequestBlockedPrivateAddr {
            label: label.to_string(),
        });
    }
    crate::ssrf::check_ip_not_private(peer.ip(), "openapi peer").map_err(|_| {
        OpenApiError::RequestBlockedPrivateAddr {
            label: label.to_string(),
        }
    })?;
    Ok(())
}
