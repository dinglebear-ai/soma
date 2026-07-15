use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::snippet::store::{SnippetInfo, SnippetInputSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CodeModeCatalogKind {
    Tool,
    Snippet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeModeSnippetInputEntry {
    pub name: String,
    #[serde(flatten)]
    pub spec: SnippetInputSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub kind: CodeModeCatalogKind,
    pub id: String,
    pub name: String,
    pub namespace: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    pub signature: String,
    pub dts: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<CodeModeSnippetInputEntry>,
}

impl ToolDescriptor {
    #[must_use]
    pub fn tool(
        namespace: &str,
        tool: &str,
        description: &str,
        schema: Option<Value>,
        output_schema: Option<Value>,
    ) -> Self {
        let types = crate::ts_signatures::generate_tool_types(
            namespace,
            tool,
            description,
            schema.as_ref(),
            output_schema.as_ref(),
        );
        Self {
            kind: CodeModeCatalogKind::Tool,
            id: super::id::namespaced_tool_id(namespace, tool),
            name: tool.to_string(),
            namespace: namespace.to_string(),
            description: description.to_string(),
            schema,
            output_schema,
            signature: types.signature,
            dts: types.dts,
            tags: Vec::new(),
            inputs: Vec::new(),
        }
    }

    #[must_use]
    pub fn snippet(info: &SnippetInfo) -> Self {
        let description = info
            .description
            .clone()
            .unwrap_or_else(|| format!("Code Mode snippet `{}`", info.name));
        let inputs = info
            .inputs
            .iter()
            .map(|(name, spec)| CodeModeSnippetInputEntry {
                name: name.clone(),
                spec: spec.clone(),
            })
            .collect();
        Self {
            kind: CodeModeCatalogKind::Snippet,
            id: super::id::namespaced_tool_id("snippet", &info.name),
            name: info.name.clone(),
            namespace: "snippet".into(),
            description,
            schema: None,
            output_schema: None,
            signature: format!("codemode.run({:?}, input?)", info.name),
            dts: String::new(),
            tags: Vec::new(),
            inputs,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiLink {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

pub fn destructive_permitted(value: &Value) -> bool {
    value
        .get("confirm")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}
