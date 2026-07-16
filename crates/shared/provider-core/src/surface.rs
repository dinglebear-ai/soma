use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderSurface {
    #[default]
    Internal,
    Mcp,
    Rest,
    Cli,
    Palette,
    Ui,
}

impl ProviderSurface {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Internal => "internal",
            Self::Mcp => "mcp",
            Self::Rest => "rest",
            Self::Cli => "cli",
            Self::Palette => "palette",
            Self::Ui => "ui",
        }
    }
}
