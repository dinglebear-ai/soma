use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeModeSurface {
    Cli,
    Mcp,
    Api,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeModeCallerCapabilities {
    #[serde(default)]
    pub trusted_local: bool,
    #[serde(default)]
    pub admin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeModeCaller {
    pub id: String,
    #[serde(default)]
    pub capabilities: CodeModeCallerCapabilities,
}

impl CodeModeCaller {
    #[must_use]
    pub fn trusted_local(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            capabilities: CodeModeCallerCapabilities {
                trusted_local: true,
                admin: true,
            },
        }
    }
}
