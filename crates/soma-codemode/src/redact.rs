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
            | "session"
            | "session_id"
            | "cookie"
            | "code"
            | "cwd"
            | "terminal_id"
    ) || normalized.ends_with("_token")
        || normalized.ends_with("_secret")
        || normalized.ends_with("_password")
        || normalized.ends_with("_key")
}

pub fn redact_url(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut parsed) => {
            let _ = parsed.set_username("");
            let _ = parsed.set_password(None);
            parsed.set_query(parsed.query().map(redact_query_pairs).as_deref());
            parsed.set_fragment(None);
            parsed.to_string()
        }
        Err(_) => "[invalid-url-redacted]".to_string(),
    }
}

pub fn redact_stdio_value(value: &str) -> String {
    if let Some((key, _)) = value.split_once('=') {
        if is_sensitive_key(key) {
            return format!("{key}=[redacted]");
        }
    }
    if let Some(flag) = value.strip_prefix("--") {
        let (key, _) = flag
            .split_once('=')
            .map_or((flag, ""), |(key, value)| (key, value));
        if is_sensitive_key(key) {
            return format!("--{key}=[redacted]");
        }
    }
    value.to_string()
}

pub fn redact_stdio_args(args: &[String]) -> Vec<String> {
    let mut redacted = Vec::with_capacity(args.len());
    let mut redact_next = false;
    for arg in args {
        if redact_next {
            redacted.push("[redacted]".to_string());
            redact_next = false;
            continue;
        }
        let sensitive_flag = arg
            .strip_prefix("--")
            .map(|flag| flag.split_once('=').map_or(flag, |(key, _)| key))
            .is_some_and(is_sensitive_key);
        if sensitive_flag && !arg.contains('=') {
            redacted.push(arg.clone());
            redact_next = true;
        } else {
            redacted.push(redact_stdio_value(arg));
        }
    }
    redacted
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
                format!("{key}=[redacted]")
            } else if value.is_empty() {
                key.to_string()
            } else {
                format!("{key}={value}")
            }
        })
        .collect::<Vec<_>>()
        .join("&")
}
