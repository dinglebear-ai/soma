pub(super) fn string(key: &str, target: &mut String) {
    if let Ok(value) = std::env::var(key)
        && !value.is_empty()
    {
        *target = value;
    }
}

pub(super) fn optional_string(key: &str, target: &mut Option<String>) {
    if let Ok(value) = std::env::var(key)
        && !value.is_empty()
    {
        *target = Some(value);
    }
}

pub(super) fn parse<T: std::str::FromStr>(key: &str, target: &mut T) -> anyhow::Result<()> {
    if let Ok(value) = std::env::var(key)
        && !value.is_empty()
    {
        *target = value
            .parse()
            .map_err(|_| anyhow::anyhow!("{key}: invalid value {value:?}"))?;
    }
    Ok(())
}

pub(super) fn optional_parse<T: std::str::FromStr>(
    key: &str,
    target: &mut Option<T>,
) -> anyhow::Result<()> {
    if let Ok(value) = std::env::var(key)
        && !value.is_empty()
    {
        *target = Some(
            value
                .parse()
                .map_err(|_| anyhow::anyhow!("{key}: invalid value {value:?}"))?,
        );
    }
    Ok(())
}

pub(super) fn boolean(key: &str, target: &mut bool) -> anyhow::Result<()> {
    if let Ok(value) = std::env::var(key) {
        match value.to_lowercase().as_str() {
            "1" | "true" | "yes" => *target = true,
            "0" | "false" | "no" => *target = false,
            other => anyhow::bail!("{key}: expected bool, got {other:?}"),
        }
    }
    Ok(())
}

pub(super) fn list(key: &str, target: &mut Vec<String>) {
    if let Ok(value) = std::env::var(key) {
        let items: Vec<String> = value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        if !items.is_empty() {
            *target = items;
        }
    }
}

#[cfg(test)]
#[path = "env_tests.rs"]
mod tests;
