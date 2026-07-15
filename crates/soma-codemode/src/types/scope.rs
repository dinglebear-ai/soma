use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolScope {
    #[default]
    All,
    Namespaces(BTreeSet<String>),
    Tools(BTreeSet<String>),
}

impl ToolScope {
    #[must_use]
    pub fn allows(&self, id: &str) -> bool {
        match self {
            Self::All => true,
            Self::Namespaces(namespaces) => id
                .split_once("::")
                .is_some_and(|(namespace, _)| namespaces.contains(namespace)),
            Self::Tools(tools) => tools.contains(id),
        }
    }
}
