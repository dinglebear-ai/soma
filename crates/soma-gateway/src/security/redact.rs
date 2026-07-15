use serde_json::Value;
use url::Url;

const REDACTED: &str = "[redacted]";

#[must_use]
pub fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace('-', "_");
    if matches!(
        normalized.as_str(),
        "sort_key" | "cache_key" | "idempotency_key" | "partition_key" | "primary_key"
    ) {
        return false;
    }
    matches!(
        normalized.as_str(),
        "token"
            | "access_token"
            | "id_token"
            | "refresh_token"
            | "apikey"
            | "api_key"
            | "password"
            | "passwd"
            | "secret"
            | "client_secret"
            | "authorization"
            | "bearer"
            | "cookie"
            | "session"
            | "session_id"
            | "code"
    ) || normalized.ends_with("_token")
        || normalized.ends_with("_secret")
        || normalized.ends_with("_password")
        || normalized.ends_with("_key")
}

#[must_use]
pub fn redact_url(raw: &str) -> String {
    match Url::parse(raw) {
        Ok(parsed) => redact_parsed_url(parsed),
        Err(_) => redact_secret_like_segments(raw),
    }
}

#[must_use]
pub fn redact_stdio_value(value: &str) -> String {
    if let Some((key, _)) = value.split_once('=') {
        if is_sensitive_key(key) {
            return format!("{key}={REDACTED}");
        }
    }

    if let Some(flag) = value.strip_prefix("--") {
        let key = flag.split_once('=').map_or(flag, |(key, _)| key);
        if is_sensitive_key(key) {
            return format!("--{key}={REDACTED}");
        }
    }

    redact_secret_like_segments(value)
}

#[must_use]
pub fn redact_stdio_args(args: &[String]) -> Vec<String> {
    let mut redacted = Vec::with_capacity(args.len());
    let mut redact_next = false;
    for arg in args {
        if redact_next {
            redacted.push(REDACTED.to_owned());
            redact_next = false;
            continue;
        }
        let split_sensitive_flag = arg
            .strip_prefix("--")
            .map(|value| value.split_once('=').map_or(value, |(key, _)| key))
            .is_some_and(is_sensitive_key);
        if split_sensitive_flag && !arg.contains('=') {
            redacted.push(arg.clone());
            redact_next = true;
            continue;
        }
        redacted.push(redact_stdio_value(arg));
    }
    redacted
}

#[must_use]
pub fn redact_json_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    if is_sensitive_key(key) {
                        (key.clone(), Value::String(REDACTED.to_owned()))
                    } else {
                        (key.clone(), redact_json_value(value))
                    }
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(redact_json_value).collect()),
        Value::String(value) => Value::String(redact_secret_like_segments(value)),
        other => other.clone(),
    }
}

#[must_use]
pub fn redact_log_line(line: &str) -> String {
    let mut out = Vec::new();
    let mut auth_tokens_to_redact = 0usize;
    for token in line.split_whitespace() {
        if auth_tokens_to_redact > 0 {
            out.push(REDACTED.to_owned());
            auth_tokens_to_redact -= 1;
            continue;
        }
        if token
            .trim_end_matches(':')
            .eq_ignore_ascii_case("authorization")
        {
            out.push("Authorization:".to_owned());
            auth_tokens_to_redact = 2;
            continue;
        }
        out.push(redact_stdio_value(token));
    }
    out.join(" ")
}

fn redact_parsed_url(mut parsed: Url) -> String {
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    let query = parsed.query().map(redact_query_pairs);
    parsed.set_query(query.as_deref());
    parsed.set_fragment(None);
    parsed.to_string()
}

fn redact_query_pairs(query: &str) -> String {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair
                .split_once('=')
                .map_or((pair, ""), |(key, value)| (key, value));
            if is_sensitive_key(key) {
                format!("{key}={REDACTED}")
            } else if value.is_empty() {
                key.to_owned()
            } else {
                format!("{key}={value}")
            }
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn redact_secret_like_segments(value: &str) -> String {
    value
        .split_whitespace()
        .map(|segment| {
            if looks_secret_like(segment) {
                REDACTED.to_owned()
            } else {
                segment.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn looks_secret_like(value: &str) -> bool {
    let trimmed = value.trim_matches(|ch: char| matches!(ch, '"' | '\'' | ',' | ';'));
    trimmed.starts_with("Bearer ")
        || trimmed.starts_with("sk-")
        || trimmed.starts_with("ghp_")
        || trimmed.starts_with("github_pat_")
        || looks_like_jwt(trimmed)
}

fn looks_like_jwt(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 3 && parts.iter().all(|part| part.len() >= 8)
}

#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;
