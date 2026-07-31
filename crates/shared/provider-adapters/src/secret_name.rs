pub(crate) fn environment_name(handle: &str) -> Result<String, String> {
    if handle.is_empty()
        || !handle
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(
            "secret handles must use lowercase ASCII letters, digits, and hyphens".to_owned(),
        );
    }
    Ok(format!(
        "SOMA_PROVIDER_SECRET_{}",
        handle.replace('-', "_").to_ascii_uppercase()
    ))
}

pub(crate) fn redact(message: &str, secret_names: &[String]) -> String {
    let mut secret_values = secret_names
        .iter()
        .filter_map(|name| environment_name(name).ok())
        .filter_map(|variable| std::env::var(variable).ok())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    secret_values.sort_by_key(|value| std::cmp::Reverse(value.len()));
    secret_values.dedup();
    redact_secret_values(message, &secret_values)
}

fn redact_secret_values(message: &str, secret_values: &[String]) -> String {
    // Include enough source text to capture any secret that starts inside the
    // public 1,024-character window. Truncating to that window first can expose
    // a prefix, while cloning the entire bounded runner frame would make each
    // replacement needlessly scan megabytes of text.
    let max_secret_chars = secret_values
        .iter()
        .map(|value| value.chars().count())
        .max()
        .unwrap_or(0);
    let scan_chars = 1_024usize.saturating_add(max_secret_chars.saturating_sub(1));
    let mut public = message.chars().take(scan_chars).collect::<String>();
    // Longest-first replacement prevents an overlapping shorter value from
    // leaving the remainder of a longer secret visible.
    for value in secret_values {
        public = public.replace(value, "[redacted]");
    }
    public = public.chars().take(1_024).collect();

    let lower = public.to_ascii_lowercase();
    if [
        "token",
        "secret",
        "password",
        "authorization",
        "api_key",
        "apikey",
        "credential",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return "[redacted]".to_owned();
    }
    public
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_handles_have_one_canonical_environment_mapping() {
        assert_eq!(
            environment_name("billing-key").unwrap(),
            "SOMA_PROVIDER_SECRET_BILLING_KEY"
        );
        for alias in ["billing_key", "billing.key", "Billing-key"] {
            assert!(environment_name(alias).is_err());
        }
    }

    #[test]
    fn redacts_complete_long_secrets_before_bounding_diagnostics() {
        let secret = "x".repeat(2_048);
        let message = format!("prefix-{secret}-suffix");

        assert_eq!(
            redact_secret_values(&message, &[secret]),
            "prefix-[redacted]-suffix"
        );
    }

    #[test]
    fn redacts_overlapping_secrets_longest_first() {
        let mut values = vec!["abc".to_owned(), "abcdef".to_owned()];
        values.sort_by_key(|value| std::cmp::Reverse(value.len()));

        assert_eq!(
            redact_secret_values("value=abcdef", &values),
            "value=[redacted]"
        );
    }

    #[test]
    fn bounds_public_diagnostics_after_redaction() {
        let public = redact_secret_values(&"a".repeat(1_200), &[]);
        assert_eq!(public.chars().count(), 1_024);
    }
}
