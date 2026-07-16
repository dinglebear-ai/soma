use std::path::PathBuf;

#[must_use]
pub fn soma_home() -> PathBuf {
    if let Ok(home) = std::env::var("SOMA_HOME") {
        if !home.is_empty() {
            return PathBuf::from(home);
        }
    }
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => PathBuf::from(home).join(".soma"),
        _ => PathBuf::from(".soma"),
    }
}

#[must_use]
pub fn home_dir() -> Option<PathBuf> {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => Some(PathBuf::from(home)),
        _ => None,
    }
}

#[must_use]
pub fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}
