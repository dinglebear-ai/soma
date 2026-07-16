use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(value: impl Into<String>) -> Result<Self, ProviderIdError> {
        let value = value.into();
        if valid_id(&value) {
            Ok(Self(value))
        } else {
            Err(ProviderIdError(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for ProviderId {
    type Err = ProviderIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error("invalid provider id `{0}`; expected a lowercase provider name")]
pub struct ProviderIdError(String);

fn valid_id(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('a'..='z'))
        && chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '-' | '_'))
        && !value.ends_with(['-', '_'])
        && !value.contains("--")
        && !value.contains("__")
        && !value.contains("-_")
        && !value.contains("_-")
}
