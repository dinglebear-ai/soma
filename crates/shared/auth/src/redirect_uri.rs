//! Redirect-URI trust checks shared by DCR and CIMD client resolution.

use std::net::IpAddr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RedirectUriKind {
    Https,
    Loopback,
    PrivateUse,
}

const FORBIDDEN_PRIVATE_SCHEMES: &[&str] = &[
    "data",
    "file",
    "ftp",
    "http",
    "https",
    "intent",
    "javascript",
    "mailto",
    "tel",
    "vbscript",
];

/// Parse and validate the security-relevant shape of a registered redirect URI.
pub(crate) fn redirect_uri_kind(value: &str) -> Option<RedirectUriKind> {
    let url = reqwest::Url::parse(value).ok()?;
    if url.fragment().is_some() || !url.username().is_empty() || url.password().is_some() {
        return None;
    }

    match url.scheme() {
        "https" if url.host_str().is_some() => Some(RedirectUriKind::Https),
        "http" => {
            let host = url.host_str()?;
            let ip: IpAddr = host.parse().ok()?;
            ip.is_loopback().then_some(RedirectUriKind::Loopback)
        }
        scheme
            if !FORBIDDEN_PRIVATE_SCHEMES.contains(&scheme)
                && !url.path().is_empty()
                && !value.chars().any(char::is_control) =>
        {
            Some(RedirectUriKind::PrivateUse)
        }
        _ => None,
    }
}

pub(crate) fn is_allowed_redirect_uri(value: &str, patterns: &[String]) -> bool {
    let Some(kind) = redirect_uri_kind(value) else {
        return false;
    };
    if matches!(
        kind,
        RedirectUriKind::Loopback | RedirectUriKind::PrivateUse
    ) {
        return true;
    }

    let Ok(candidate) = reqwest::Url::parse(value) else {
        return false;
    };
    patterns
        .iter()
        .any(|pattern| redirect_pattern_matches(pattern, &candidate))
}

pub(crate) fn is_allowed_redirect_uri_for_application(
    value: &str,
    patterns: &[String],
    application_type: &str,
) -> bool {
    let Some(kind) = redirect_uri_kind(value) else {
        return false;
    };
    match application_type {
        "web" if kind != RedirectUriKind::Https => return false,
        "native" => {}
        _ if application_type != "web" => return false,
        _ => {}
    }
    is_allowed_redirect_uri(value, patterns)
}

pub(crate) fn infer_application_type(
    redirect_uris: &[String],
    native_callback: &str,
) -> &'static str {
    if redirect_uris.iter().any(|uri| {
        uri == native_callback
            || matches!(
                redirect_uri_kind(uri),
                Some(RedirectUriKind::Loopback | RedirectUriKind::PrivateUse)
            )
    }) {
        "native"
    } else {
        "web"
    }
}

pub(crate) fn wildcard_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == value;
    }

    let anchored_start = !pattern.starts_with('*');
    let anchored_end = !pattern.ends_with('*');
    let non_empty_parts: Vec<&str> = parts.into_iter().filter(|part| !part.is_empty()).collect();
    if non_empty_parts.is_empty() {
        return true;
    }

    let mut cursor = 0usize;
    for (index, part) in non_empty_parts.iter().enumerate() {
        if index == 0 && anchored_start {
            if !value[cursor..].starts_with(part) {
                return false;
            }
            cursor += part.len();
            continue;
        }

        match value[cursor..].find(part) {
            Some(found) => cursor += found + part.len(),
            None => return false,
        }
    }

    if anchored_end && let Some(last) = non_empty_parts.last() {
        return value.ends_with(last);
    }

    true
}

fn redirect_pattern_matches(pattern: &str, candidate: &reqwest::Url) -> bool {
    if pattern == "https://*" {
        return candidate.scheme() == "https" && candidate.host_str().is_some();
    }

    let Ok(pattern_url) = reqwest::Url::parse(pattern) else {
        return false;
    };
    if pattern_url.fragment().is_some()
        || !pattern_url.username().is_empty()
        || pattern_url.password().is_some()
        || pattern_url.scheme() != candidate.scheme()
    {
        return false;
    }

    if pattern_url.host_str().is_none() || candidate.host_str().is_none() {
        return wildcard_matches(pattern, candidate.as_str());
    }

    if pattern_url.port_or_known_default() != candidate.port_or_known_default() {
        return false;
    }
    let Some(pattern_host) = pattern_url.host_str() else {
        return false;
    };
    let Some(candidate_host) = candidate.host_str() else {
        return false;
    };
    if !host_pattern_matches(pattern_host, candidate_host) {
        return false;
    }
    if !wildcard_matches(pattern_url.path(), candidate.path()) {
        return false;
    }

    match (pattern_url.query(), candidate.query()) {
        (Some(pattern_query), Some(candidate_query)) => {
            wildcard_matches(pattern_query, candidate_query)
        }
        (None, None) => true,
        _ => false,
    }
}

pub(crate) fn host_pattern_matches(pattern_host: &str, candidate_host: &str) -> bool {
    let pattern_labels = pattern_host.split('.').collect::<Vec<_>>();
    let candidate_labels = candidate_host.split('.').collect::<Vec<_>>();
    if pattern_labels.len() != candidate_labels.len() {
        return false;
    }

    pattern_labels
        .iter()
        .zip(candidate_labels.iter())
        .all(|(pattern, candidate)| {
            *pattern == "*" || (!pattern.contains('*') && pattern.eq_ignore_ascii_case(candidate))
        })
}

#[cfg(test)]
mod tests {
    use super::{
        RedirectUriKind, infer_application_type, is_allowed_redirect_uri,
        is_allowed_redirect_uri_for_application, redirect_uri_kind,
    };

    #[test]
    fn accepts_secure_web_loopback_and_reverse_domain_native_redirects() {
        assert_eq!(
            redirect_uri_kind("https://client.example/callback"),
            Some(RedirectUriKind::Https)
        );
        assert_eq!(
            redirect_uri_kind("http://127.0.0.1:7777/callback"),
            Some(RedirectUriKind::Loopback)
        );
        assert_eq!(
            redirect_uri_kind("com.example.app:/oauth/callback"),
            Some(RedirectUriKind::PrivateUse)
        );
        assert!(is_allowed_redirect_uri(
            "com.example.app:/oauth/callback",
            &[]
        ));
    }

    #[test]
    fn rejects_fragments_userinfo_non_loopback_http_and_dangerous_schemes() {
        for uri in [
            "https://client.example/callback#fragment",
            "https://user:pass@client.example/callback",
            "http://localhost:7777/callback",
            "http://192.168.1.5/callback",
            "javascript:alert(1)",
            "data:text/html,hello",
            "file:///tmp/callback",
            "mailto:user@example.com",
            "ftp://example.com/callback",
            "intent://callback",
        ] {
            assert!(redirect_uri_kind(uri).is_none(), "{uri}");
        }
    }

    #[test]
    fn safe_private_schemes_remain_compatible_with_native_clients() {
        assert!(is_allowed_redirect_uri("raycast://oauth/callback", &[]));
        assert!(is_allowed_redirect_uri("warp://mcp/oauth2callback", &[]));
    }

    #[test]
    fn application_type_constrains_redirect_kind() {
        let wildcard = vec!["https://*".to_string()];
        assert!(is_allowed_redirect_uri_for_application(
            "https://client.example/callback",
            &wildcard,
            "web"
        ));
        assert!(!is_allowed_redirect_uri_for_application(
            "http://127.0.0.1:7777/callback",
            &[],
            "web"
        ));
        assert!(is_allowed_redirect_uri_for_application(
            "http://127.0.0.1:7777/callback",
            &[],
            "native"
        ));
    }

    #[test]
    fn infers_native_for_loopback_private_use_and_server_native_callback() {
        assert_eq!(
            infer_application_type(&["http://127.0.0.1:7777/cb".into()], "https://as/native"),
            "native"
        );
        assert_eq!(
            infer_application_type(&["https://as/native".into()], "https://as/native"),
            "native"
        );
        assert_eq!(
            infer_application_type(&["https://client.example/cb".into()], "https://as/native"),
            "web"
        );
    }
}
