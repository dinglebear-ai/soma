//! Unit tests for the deterministic host/origin computation logic. All tests
//! are synchronous and need no network access.

use super::*;

fn hosts_input<'a>(
    bind_host: &'a str,
    port: u16,
    extra_hosts: &'a [String],
) -> AllowedHostsInput<'a> {
    AllowedHostsInput {
        bind_host,
        port,
        extra_hosts,
        public_url: None,
        public_url_label: "public_url",
    }
}

// ── allowed_hosts ─────────────────────────────────────────────────────────────

#[test]
fn allowed_hosts_always_includes_loopback() {
    let hosts = allowed_hosts(hosts_input("0.0.0.0", 3000, &[]));
    assert!(hosts.contains(&"localhost".to_string()));
    assert!(hosts.contains(&"127.0.0.1".to_string()));
}

#[test]
fn allowed_hosts_includes_bound_host_and_port_variant() {
    let hosts = allowed_hosts(hosts_input("myhost.example.com", 8080, &[]));
    assert!(hosts.contains(&"myhost.example.com".to_string()));
    assert!(hosts.contains(&"myhost.example.com:8080".to_string()));
}

#[test]
fn allowed_hosts_deduplicates() {
    let hosts = allowed_hosts(hosts_input("localhost", 3000, &[]));
    let localhost_count = hosts.iter().filter(|h| h.as_str() == "localhost").count();
    assert_eq!(localhost_count, 1, "localhost should appear exactly once");
}

#[test]
fn allowed_hosts_with_extra_allowed_hosts() {
    let extra = vec!["proxy.internal".to_string()];
    let hosts = allowed_hosts(hosts_input("0.0.0.0", 3000, &extra));
    assert!(hosts.contains(&"proxy.internal".to_string()));
    assert!(hosts.contains(&"proxy.internal:3000".to_string()));
}

#[test]
fn allowed_hosts_ipv6_loopback_bracketed() {
    let hosts = allowed_hosts(hosts_input("0.0.0.0", 3000, &[]));
    assert!(
        hosts.iter().any(|h| h.contains("::1")),
        "IPv6 loopback should be present"
    );
}

#[test]
fn host_with_port_not_doubled() {
    // "proxy:9000" already has a port — push_host_variants must not append another.
    let extra = vec!["proxy:9000".to_string()];
    let hosts = allowed_hosts(hosts_input("0.0.0.0", 3000, &extra));
    assert!(hosts.contains(&"proxy:9000".to_string()));
    assert!(
        !hosts.contains(&"proxy:9000:3000".to_string()),
        "port must not be appended twice"
    );
}

#[test]
fn allowed_hosts_includes_public_url_host_with_explicit_port() {
    let hosts = allowed_hosts(AllowedHostsInput {
        bind_host: "0.0.0.0",
        port: 3000,
        extra_hosts: &[],
        public_url: Some("https://mcp.example.com:8443"),
        public_url_label: "public_url",
    });
    assert!(hosts.contains(&"mcp.example.com:8443".to_string()));
}

#[test]
fn allowed_hosts_includes_public_url_host_with_scheme_default_port() {
    let hosts = allowed_hosts(AllowedHostsInput {
        bind_host: "0.0.0.0",
        port: 3000,
        extra_hosts: &[],
        public_url: Some("https://mcp.example.com"),
        public_url_label: "public_url",
    });
    assert!(hosts.contains(&"mcp.example.com".to_string()));
    assert!(hosts.contains(&"mcp.example.com:443".to_string()));
}

#[test]
fn allowed_hosts_falls_back_to_listen_port_for_non_http_scheme() {
    let hosts = allowed_hosts(AllowedHostsInput {
        bind_host: "0.0.0.0",
        port: 3000,
        extra_hosts: &[],
        public_url: Some("ws://mcp.example.com"),
        public_url_label: "public_url",
    });
    assert!(hosts.contains(&"mcp.example.com:3000".to_string()));
}

#[test]
fn allowed_hosts_skips_invalid_public_url() {
    let hosts = allowed_hosts(AllowedHostsInput {
        bind_host: "0.0.0.0",
        port: 3000,
        extra_hosts: &[],
        public_url: Some("not-a-url"),
        public_url_label: "public_url",
    });
    // Invalid public_url must not panic and must not corrupt the baseline set.
    assert!(hosts.contains(&"localhost".to_string()));
}

#[test]
fn allowed_hosts_skips_wildcard_public_url_host() {
    let hosts = allowed_hosts(AllowedHostsInput {
        bind_host: "0.0.0.0",
        port: 3000,
        extra_hosts: &[],
        public_url: Some("https://*.example.com"),
        public_url_label: "public_url",
    });
    assert!(!hosts.iter().any(|h| h.contains('*')));
}

// ── allowed_origins ───────────────────────────────────────────────────────────

fn origins_input<'a>(port: u16, extra_origins: &'a [String]) -> AllowedOriginsInput<'a> {
    AllowedOriginsInput {
        port,
        extra_origins,
        public_url: None,
        extra_origins_label: "extra_origins",
        public_url_label: "public_url",
    }
}

#[test]
fn allowed_origins_includes_loopback_with_port() {
    let origins = allowed_origins(origins_input(4000, &[]));
    assert!(origins.contains(&"http://localhost:4000".to_string()));
    assert!(origins.contains(&"http://127.0.0.1:4000".to_string()));
}

#[test]
fn allowed_origins_deduplicates() {
    let origins = allowed_origins(origins_input(4000, &[]));
    let count = origins
        .iter()
        .filter(|o| o.as_str() == "http://localhost:4000")
        .count();
    assert_eq!(count, 1);
}

#[test]
fn allowed_origins_includes_extra_allowed_origins() {
    let extra = vec!["https://app.example.com".to_string()];
    let origins = allowed_origins(origins_input(3000, &extra));
    assert!(origins.contains(&"https://app.example.com".to_string()));
}

#[test]
fn allowed_origins_normalizes_extra_allowed_origins() {
    let extra = vec!["https://app.example.com/some/path?ignored=true".to_string()];
    let origins = allowed_origins(origins_input(3000, &extra));
    assert!(origins.contains(&"https://app.example.com".to_string()));
    assert!(!origins.contains(&"https://app.example.com/some/path?ignored=true".to_string()));
}

#[test]
fn allowed_origins_skips_invalid_and_wildcard_origins() {
    let extra = vec!["not-a-url".to_string(), "https://*.example.com".to_string()];
    let origins = allowed_origins(origins_input(3000, &extra));
    assert!(!origins.contains(&"not-a-url".to_string()));
    assert!(!origins.contains(&"https://*.example.com".to_string()));
}

#[test]
fn allowed_origins_preserves_non_http_configured_origins() {
    let extra = vec!["vscode-webview://extension.example".to_string()];
    let origins = allowed_origins(origins_input(3000, &extra));
    assert!(origins.contains(&"vscode-webview://extension.example".to_string()));
}

#[test]
fn allowed_origins_brackets_ipv6_literals() {
    let extra = vec!["http://[::1]:3000/path?ignored=true".to_string()];
    let origins = allowed_origins(origins_input(3000, &extra));
    assert!(origins.contains(&"http://[::1]:3000".to_string()));
    assert!(!origins.contains(&"http://::1:3000".to_string()));
}

#[test]
fn allowed_origins_includes_public_url_origin() {
    let origins = allowed_origins(AllowedOriginsInput {
        port: 3000,
        extra_origins: &[],
        public_url: Some("https://mcp.example.com"),
        extra_origins_label: "extra_origins",
        public_url_label: "public_url",
    });
    assert!(origins.contains(&"https://mcp.example.com".to_string()));
}
