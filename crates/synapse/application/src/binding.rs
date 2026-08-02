use serde::{Deserialize, Serialize};
use soma_ops::OperationName;

/// Legacy Synapse MCP tool family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyTool {
    /// Flux Docker and host operations.
    Flux,
    /// Scout filesystem and SSH operations.
    Scout,
    /// Binding shared by both tools, currently only help.
    Both,
}

impl LegacyTool {
    /// Returns the legacy MCP tool name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Flux => "flux",
            Self::Scout => "scout",
            Self::Both => "both",
        }
    }
}

/// Legacy Synapse authorization classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyAccess {
    /// Public help operation.
    Public,
    /// Requires the read scope.
    Read,
    /// Requires the write scope.
    Write,
}

/// Legacy transport exposure recorded by the donor registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyTransport {
    /// Exposed through REST and MCP compatibility surfaces.
    Rest,
    /// Exposed only through MCP compatibility surfaces.
    McpOnly,
}

/// Legacy response presentation requested by Flux or Scout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LegacyPresentation {
    /// Human-readable Markdown.
    #[default]
    Markdown,
    /// Structured JSON.
    Json,
}

/// Product-owned binding from one legacy route to a canonical operation.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LegacyOperationBinding {
    legacy_name: String,
    canonical_name: OperationName,
    legacy_tool: LegacyTool,
    legacy_action: String,
    legacy_subaction: Option<String>,
    legacy_access: LegacyAccess,
    legacy_scope: Option<String>,
    legacy_destructive: bool,
    legacy_transport: LegacyTransport,
    required_params: Vec<String>,
    required_any: Vec<Vec<String>>,
}

impl LegacyOperationBinding {
    /// Returns the historical operation name.
    #[must_use]
    pub fn legacy_name(&self) -> &str {
        &self.legacy_name
    }
    /// Returns the canonical operation name.
    #[must_use]
    pub fn canonical_name(&self) -> &OperationName {
        &self.canonical_name
    }
    /// Returns the legacy tool owner.
    #[must_use]
    pub const fn tool(&self) -> LegacyTool {
        self.legacy_tool
    }
    /// Returns the legacy action.
    #[must_use]
    pub fn action(&self) -> &str {
        &self.legacy_action
    }
    /// Returns the optional legacy subaction.
    #[must_use]
    pub fn subaction(&self) -> Option<&str> {
        self.legacy_subaction.as_deref()
    }
    /// Returns the legacy authorization class.
    #[must_use]
    pub const fn access(&self) -> LegacyAccess {
        self.legacy_access
    }
    /// Returns the product authorization scope.
    #[must_use]
    pub fn scope(&self) -> Option<&str> {
        self.legacy_scope.as_deref()
    }
    /// Returns the historical destructive flag.
    #[must_use]
    pub const fn destructive(&self) -> bool {
        self.legacy_destructive
    }
    /// Returns historical transport exposure.
    #[must_use]
    pub const fn transport(&self) -> LegacyTransport {
        self.legacy_transport
    }
    /// Returns required legacy fields.
    #[must_use]
    pub fn required_params(&self) -> &[String] {
        &self.required_params
    }
    /// Returns alternative complete legacy field groups.
    #[must_use]
    pub fn required_any(&self) -> &[Vec<String>] {
        &self.required_any
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct LegacyBindingKey {
    pub(crate) tool: LegacyTool,
    pub(crate) action: String,
    pub(crate) subaction: Option<String>,
}

impl LegacyBindingKey {
    pub(crate) fn new(tool: LegacyTool, action: &str, subaction: Option<&str>) -> Self {
        Self {
            tool,
            action: action.to_owned(),
            subaction: subaction.map(str::to_owned),
        }
    }
}

#[cfg(test)]
#[path = "binding_tests.rs"]
mod tests;
