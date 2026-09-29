use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestActor {
    pub surface: String,
    pub display: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

impl RequestActor {
    pub fn new(surface: impl Into<String>, display: impl Into<String>) -> Self {
        Self {
            surface: surface.into(),
            display: display.into(),
            subject: None,
            email: None,
        }
    }

    pub fn api() -> Self {
        Self::new("api", "api")
    }

    pub fn cli() -> Self {
        Self::new("cli", "cli")
    }

    pub fn mcp_loopback() -> Self {
        Self::new("mcp", "mcp:loopback")
    }

    pub fn mcp_bearer() -> Self {
        Self::new("mcp", "mcp:bearer")
    }

    pub fn mcp_oauth() -> Self {
        Self::new("mcp", "mcp:oauth")
    }

    pub fn mcp_identity(subject: Option<String>, email: Option<String>) -> Self {
        let display = email
            .as_deref()
            .filter(|value| !value.is_empty())
            .or_else(|| subject.as_deref().filter(|value| !value.is_empty()))
            .unwrap_or("mcp:oauth")
            .to_string();
        Self {
            surface: "mcp".to_string(),
            display,
            subject,
            email,
        }
    }
}

impl From<&str> for RequestActor {
    fn from(value: &str) -> Self {
        Self::new("unknown", value)
    }
}

impl From<String> for RequestActor {
    fn from(value: String) -> Self {
        Self::new("unknown", value)
    }
}

#[cfg(test)]
#[path = "actor_tests.rs"]
mod tests;
