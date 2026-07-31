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
    let mut public = message.chars().take(1024).collect::<String>();
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
    for name in secret_names {
        if let Ok(variable) = environment_name(name)
            && let Ok(value) = std::env::var(variable)
            && !value.is_empty()
        {
            public = public.replace(&value, "[redacted]");
        }
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
}
