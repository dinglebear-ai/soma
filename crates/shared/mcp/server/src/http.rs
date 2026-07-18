//! Reusable inbound HTTP MCP transport lifecycle helpers.
//!
//! Computes the RMCP Streamable HTTP allow-listed `Host`/`Origin` values and
//! builds the transport config/service from primitive bind/config values —
//! no product config type required. A product adapter owns extracting those
//! primitives from its own config struct; this module owns the deterministic
//! computation and the RMCP transport wiring.

use std::net::Ipv6Addr;

use rmcp::{
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ServerHandler,
};

// ── allowed hosts ─────────────────────────────────────────────────────────────

/// Inputs needed to compute the allow-listed `Host` header values for an
/// inbound MCP HTTP server.
#[derive(Clone, Copy, Debug)]
pub struct AllowedHostsInput<'a> {
    pub bind_host: &'a str,
    pub port: u16,
    pub extra_hosts: &'a [String],
    pub public_url: Option<&'a str>,
    /// Label used in diagnostic logs when `public_url` fails to parse or
    /// contains a wildcard host — typically the product's env var name (for
    /// example `SOMA_MCP_PUBLIC_URL`) so operators can trace a warning back
    /// to the setting that produced it. Pass a generic label such as
    /// `"public_url"` if no product-specific name applies.
    pub public_url_label: &'a str,
}

pub fn allowed_hosts(input: AllowedHostsInput<'_>) -> Vec<String> {
    let mut hosts = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    push_host_variants(&mut hosts, input.bind_host, input.port);
    push_host_variants(&mut hosts, "localhost", input.port);
    push_host_variants(&mut hosts, "127.0.0.1", input.port);
    push_host_variants(&mut hosts, "::1", input.port);
    for host in input.extra_hosts {
        push_host_variants(&mut hosts, host, input.port);
    }
    if let Some(public_url) = input.public_url {
        push_public_url_hosts(&mut hosts, public_url, input.port, input.public_url_label);
    }
    hosts.sort();
    hosts.dedup();
    hosts
}

// ── allowed origins ───────────────────────────────────────────────────────────

/// Inputs needed to compute the allow-listed `Origin` header values for an
/// inbound MCP HTTP server.
#[derive(Clone, Copy, Debug)]
pub struct AllowedOriginsInput<'a> {
    pub port: u16,
    pub extra_origins: &'a [String],
    pub public_url: Option<&'a str>,
    /// Label used in diagnostic logs for a rejected entry in `extra_origins`
    /// — typically the product's env var name (for example
    /// `SOMA_MCP_ALLOWED_ORIGINS`). Pass a generic label such as
    /// `"extra_origins"` if no product-specific name applies.
    pub extra_origins_label: &'a str,
    /// Label used in diagnostic logs when `public_url` fails to parse —
    /// typically the product's env var name (for example
    /// `SOMA_MCP_PUBLIC_URL`). Pass a generic label such as `"public_url"`
    /// if no product-specific name applies.
    pub public_url_label: &'a str,
}

pub fn allowed_origins(input: AllowedOriginsInput<'_>) -> Vec<String> {
    let mut origins = vec![
        format!("http://localhost:{}", input.port),
        format!("http://127.0.0.1:{}", input.port),
    ];
    for origin in input.extra_origins {
        push_configured_origin(&mut origins, origin, input.extra_origins_label);
    }
    if let Some(public_url) = input.public_url {
        if let Some(origin) = extract_origin_with_label(public_url, input.public_url_label) {
            origins.push(origin);
        }
    }
    origins.sort();
    origins.dedup();
    origins
}

// ── transport builders ────────────────────────────────────────────────────────

pub fn streamable_http_config(
    hosts: Vec<String>,
    origins: Vec<String>,
) -> StreamableHttpServerConfig {
    StreamableHttpServerConfig::default()
        .with_stateful_mode(false)
        .with_json_response(true)
        .with_allowed_hosts(hosts)
        .with_allowed_origins(origins)
}

/// Build a [`StreamableHttpService`] for any [`ServerHandler`], given a
/// factory that produces a fresh handler per session and a transport config
/// (see [`streamable_http_config`]).
pub fn streamable_http_service<S, F>(
    factory: F,
    config: StreamableHttpServerConfig,
) -> StreamableHttpService<S, LocalSessionManager>
where
    S: ServerHandler,
    F: Fn() -> Result<S, std::io::Error> + Send + Sync + 'static,
{
    StreamableHttpService::new(factory, Default::default(), config)
}

// ── private helpers ───────────────────────────────────────────────────────────

fn push_configured_origin(origins: &mut Vec<String>, origin: &str, label: &str) {
    let Some(origin) = extract_configured_origin_with_label(origin, label) else {
        return;
    };
    origins.push(origin);
}

fn push_host_variants(hosts: &mut Vec<String>, host: &str, port: u16) {
    let host = host.trim();
    if host.is_empty() {
        return;
    }
    hosts.push(host.to_string());
    if host.starts_with('[') && host.contains("]:") {
        return;
    }
    if let Some(inner) = host.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
        if !inner.is_empty() {
            hosts.push(format!("[{inner}]:{port}"));
        }
    } else if host.parse::<Ipv6Addr>().is_ok() {
        hosts.push(format!("[{host}]"));
        hosts.push(format!("[{host}]:{port}"));
    } else if !has_port(host) {
        hosts.push(format!("{host}:{port}"));
    }
}

fn push_public_url_hosts(hosts: &mut Vec<String>, url: &str, listen_port: u16, label: &str) {
    let Ok(parsed) = url::Url::parse(url) else {
        tracing::warn!(
            setting = label,
            public_url = url,
            "MCP public URL is not a valid URL"
        );
        return;
    };
    let Some(host) = parsed.host_str() else {
        return;
    };
    if host.contains('*') {
        tracing::warn!(
            setting = label,
            host,
            "MCP public URL host contains wildcard; skipping"
        );
        return;
    }
    let explicit_port = parsed.port();
    let scheme_default = match parsed.scheme() {
        "https" => Some(443u16),
        "http" => Some(80u16),
        _ => None,
    };
    if let Some(p) = explicit_port {
        push_host_variants(hosts, host, p);
        let with_port = format!("{host}:{p}");
        if !hosts.contains(&with_port) {
            hosts.push(with_port);
        }
    } else if let Some(default_port) = scheme_default {
        let bare = host.to_string();
        if !hosts.contains(&bare) {
            hosts.push(bare);
        }
        let with_default = format!("{host}:{default_port}");
        if !hosts.contains(&with_default) {
            hosts.push(with_default);
        }
    } else {
        push_host_variants(hosts, host, listen_port);
    }
}

fn has_port(host: &str) -> bool {
    host.rsplit_once(':')
        .and_then(|(_, p)| p.parse::<u16>().ok())
        .is_some()
}

fn extract_origin_with_label(url: &str, label: &str) -> Option<String> {
    let parsed = url::Url::parse(url)
        .map_err(|e| tracing::warn!(setting = label, url, error = %e, "invalid MCP origin URL"))
        .ok()?;
    let scheme = parsed.scheme();
    let host = parsed.host()?;
    let host_text = format_origin_host(host);
    if host_text.contains('*') {
        tracing::warn!(
            setting = label,
            host = %host_text,
            "MCP origin host contains wildcard; skipping"
        );
        return None;
    }
    let default_port = match scheme {
        "http" => Some(80u16),
        "https" => Some(443u16),
        _ => {
            tracing::warn!(
                setting = label,
                scheme,
                "MCP origin URL must use http or https"
            );
            return None;
        }
    };
    let origin = match parsed.port() {
        Some(port) if default_port != Some(port) => format!("{scheme}://{host_text}:{port}"),
        _ => format!("{scheme}://{host_text}"),
    };
    Some(origin)
}

fn extract_configured_origin_with_label(url: &str, label: &str) -> Option<String> {
    match extract_origin_with_label(url, label) {
        Some(origin) => Some(origin),
        None => {
            let parsed = url::Url::parse(url).ok()?;
            if matches!(parsed.scheme(), "http" | "https") {
                return None;
            }
            Some(url.trim().to_string())
        }
    }
}

fn format_origin_host(host: url::Host<&str>) -> String {
    match host {
        url::Host::Domain(domain) => domain.to_string(),
        url::Host::Ipv4(addr) => addr.to_string(),
        url::Host::Ipv6(addr) => format!("[{addr}]"),
    }
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
