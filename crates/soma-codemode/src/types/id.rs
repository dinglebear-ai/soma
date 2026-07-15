use crate::{util::invalid_code_mode_id, ToolError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeModeToolId {
    pub raw: String,
    pub reference: CodeModeToolRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeModeToolRef {
    Tool { namespace: String, tool: String },
}

impl CodeModeToolId {
    pub fn parse(raw: &str) -> Result<Self, ToolError> {
        raw.parse()
    }
}

impl std::str::FromStr for CodeModeToolId {
    type Err = ToolError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(invalid_code_mode_id("Code Mode tool id must not be empty"));
        }
        let Some((namespace, tool)) = split_namespaced_id(raw) else {
            return Err(invalid_code_mode_id(
                "Code Mode ids must use <namespace>::<tool>",
            ));
        };
        Ok(Self {
            raw: raw.to_string(),
            reference: CodeModeToolRef::Tool {
                namespace: namespace.to_string(),
                tool: tool.to_string(),
            },
        })
    }
}

pub fn split_namespaced_id(raw: &str) -> Option<(&str, &str)> {
    let mut parts = raw.split("::");
    let namespace = parts.next()?.trim();
    let tool = parts.next()?.trim();
    if parts.next().is_some() || namespace.is_empty() || tool.is_empty() {
        return None;
    }
    Some((namespace, tool))
}

#[must_use]
pub fn namespaced_tool_id(namespace: &str, tool: &str) -> String {
    format!("{namespace}::{tool}")
}
