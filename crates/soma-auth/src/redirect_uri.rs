//! Pure redirect-URI trust checks shared by DCR (`registration.rs`) and CIMD
//! (`authorize.rs`) client resolution. No I/O, no `AuthState` — every
//! function here takes only strings/patterns and returns a bool, so this
//! module has no feature dependency of its own beyond `reqwest::Url`
//! parsing; it's gated behind `http-axum` only because its sole callers are.

fn is_loopback_redirect(value: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    if url.scheme() != "http" {
        return false;
    }
    matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"))
}

/// Native-app private-use URI scheme redirects (RFC 8252 §7.1), e.g.
/// `com.raycast:/oauth`. Only an app registered for that scheme with the
/// OS can receive the redirect, so — like loopback — these don't need an
/// explicit allowlist entry per client. Deliberately excludes `http(s)`
/// (network-reachable, needs the allowlist) and script-executing pseudo
/// schemes a browser might act on directly instead of merely redirecting.
fn is_native_app_scheme_redirect(value: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    !matches!(
        url.scheme(),
        "http" | "https" | "javascript" | "data" | "vbscript" | "file"
    )
}

pub(crate) fn is_allowed_redirect_uri(value: &str, patterns: &[String]) -> bool {
    if is_loopback_redirect(value) || is_native_app_scheme_redirect(value) {
        return true;
    }

    let Ok(candidate) = reqwest::Url::parse(value) else {
        return false;
    };
    patterns
        .iter()
        .any(|pattern| redirect_pattern_matches(pattern, &candidate))
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
    if pattern_url.scheme() != candidate.scheme() {
        return false;
    }

    // Native-app custom URI schemes (e.g. `com.raycast:/oauth`) have no
    // authority component, so `host_str()` is None and can never satisfy the
    // host/port comparison below. Compare the whole URI instead.
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
