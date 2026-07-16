use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ProviderId;

pub type ProviderCatalog = ProviderManifest;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProviderManifest {
    pub schema_version: u32,
    pub provider: ProviderIdentity,
    #[serde(default)]
    pub tools: Vec<ProviderTool>,
    #[serde(default)]
    pub prompts: Vec<ProviderPrompt>,
    #[serde(default)]
    pub resources: Vec<ProviderResource>,
    #[serde(default)]
    pub tasks: Vec<ProviderTask>,
    #[serde(default)]
    pub elicitation: Vec<ProviderElicitation>,
    #[serde(default)]
    pub env: Vec<EnvRequirement>,
    #[serde(default)]
    pub capabilities: HostCapabilities,
    #[serde(default)]
    pub docs: Option<DocsOverlay>,
    #[serde(default)]
    pub plugin: Option<PluginOverlay>,
    #[serde(default)]
    pub ui: Option<UiOverlay>,
    #[serde(default = "default_object")]
    pub meta: Value,
}

impl ProviderManifest {
    pub fn new(id: ProviderId, title: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            schema_version: 1,
            provider: ProviderIdentity {
                name: id.into_string(),
                kind: ProviderKind::StaticRust,
                title: Some(title.into()),
                description: None,
                homepage: None,
                source: None,
                version: Some(version.into()),
                enabled: None,
            },
            tools: Vec::new(),
            prompts: Vec::new(),
            resources: Vec::new(),
            tasks: Vec::new(),
            elicitation: Vec::new(),
            env: Vec::new(),
            capabilities: HostCapabilities::default(),
            docs: None,
            plugin: None,
            ui: None,
            meta: default_object(),
        }
    }

    #[must_use]
    pub fn with_tool(mut self, tool: ToolSpec) -> Self {
        self.tools.push(tool);
        self
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProviderIdentity {
    pub name: String,
    pub kind: ProviderKind,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    StaticRust,
    Openapi,
    AiSdk,
    Wasm,
    Mcp,
    Python,
    Langchain,
    Llamaindex,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StaticRust => "static-rust",
            Self::Openapi => "openapi",
            Self::AiSdk => "ai-sdk",
            Self::Wasm => "wasm",
            Self::Mcp => "mcp",
            Self::Python => "python",
            Self::Langchain => "langchain",
            Self::Llamaindex => "llamaindex",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub title: Option<String>,
    pub input_schema: Value,
    #[serde(default)]
    pub output_schema: Option<Value>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub destructive: bool,
    #[serde(default)]
    pub requires_admin: bool,
    #[serde(default)]
    pub cost: Option<String>,
    #[serde(default)]
    pub env: Vec<EnvRequirement>,
    #[serde(default)]
    pub limits: Option<ProviderLimits>,
    #[serde(default)]
    pub mcp: Option<McpOverlay>,
    #[serde(default)]
    pub rest: Option<RestOverlay>,
    #[serde(default)]
    pub cli: Option<CliOverlay>,
    #[serde(default)]
    pub palette: Option<PaletteOverlay>,
    #[serde(default)]
    pub ui: Option<UiOverlay>,
    #[serde(default)]
    pub examples: Vec<ProviderExample>,
    #[serde(default = "default_object")]
    pub meta: Value,
}

impl ToolSpec {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            title: None,
            input_schema,
            output_schema: None,
            scope: None,
            destructive: false,
            requires_admin: false,
            cost: None,
            env: Vec::new(),
            limits: None,
            mcp: None,
            rest: None,
            cli: None,
            palette: None,
            ui: None,
            examples: Vec::new(),
            meta: default_object(),
        }
    }

    pub fn exposed_on(&self, surface: crate::ProviderSurface) -> bool {
        match surface {
            crate::ProviderSurface::Internal => true,
            crate::ProviderSurface::Mcp => self
                .mcp
                .as_ref()
                .map(|overlay| overlay.enabled)
                .unwrap_or(true),
            crate::ProviderSurface::Rest => self
                .rest
                .as_ref()
                .map(|overlay| overlay.enabled)
                .unwrap_or(true),
            crate::ProviderSurface::Cli => self
                .cli
                .as_ref()
                .map(|overlay| overlay.enabled)
                .unwrap_or(false),
            crate::ProviderSurface::Palette => self
                .palette
                .as_ref()
                .map(|overlay| overlay.enabled)
                .unwrap_or(true),
            crate::ProviderSurface::Ui => self
                .ui
                .as_ref()
                .map(|overlay| overlay.enabled)
                .unwrap_or(true),
        }
    }
}

pub type ProviderTool = ToolSpec;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProviderPrompt {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub description: String,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub arguments_schema: Option<Value>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub mcp: Option<McpOverlay>,
    #[serde(default)]
    pub examples: Vec<ProviderExample>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProviderResource {
    pub uri_template: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub mcp: Option<McpOverlay>,
    #[serde(default = "default_object")]
    pub annotations: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProviderTask {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(default)]
    pub output_schema: Option<Value>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub mcp: Option<McpOverlay>,
    #[serde(default)]
    pub limits: Option<ProviderLimits>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProviderElicitation {
    pub name: String,
    pub description: String,
    pub schema: Value,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub mcp: Option<McpOverlay>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnvRequirement {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(default = "default_true")]
    pub sensitive: bool,
    #[serde(default = "default_true")]
    pub server_prefixed: bool,
    #[serde(default)]
    pub allow_unprefixed: bool,
    #[serde(default)]
    pub default: Option<Value>,
}

impl EnvRequirement {
    pub fn runtime_name(&self, server_prefix: &str) -> String {
        if self.server_prefixed {
            format!(
                "{}_{}",
                server_prefix.trim_matches('_').to_ascii_uppercase(),
                self.name
            )
        } else {
            self.name.clone()
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HostCapabilities {
    #[serde(default)]
    pub filesystem: Option<FilesystemCapability>,
    #[serde(default)]
    pub network: Option<NetworkCapability>,
    #[serde(default)]
    pub env: Option<EnvCapability>,
    #[serde(default)]
    pub terminal: Option<TerminalCapability>,
    #[serde(default)]
    pub browser: Option<BrowserCapability>,
    #[serde(default)]
    pub github: Option<GitHubCapability>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CapabilityGrant {
    Filesystem {
        read_roots: Vec<String>,
        write_roots: Vec<String>,
    },
    Network {
        allowed_hosts: Vec<String>,
    },
    Env {
        allowed: Vec<String>,
    },
    Terminal {
        working_dir: Option<String>,
        allowlist: Vec<String>,
    },
    Browser {
        allowed_origins: Vec<String>,
    },
    Github {
        allowed_repos: Vec<String>,
        read_only: bool,
    },
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FilesystemCapability {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub read_roots: Vec<String>,
    #[serde(default)]
    pub write_roots: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NetworkCapability {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnvCapability {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allowed: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TerminalCapability {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub allowlist: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BrowserCapability {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allowed_origins: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GitHubCapability {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allowed_repos: Vec<String>,
    #[serde(default = "default_true")]
    pub read_only: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct McpOverlay {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub icons: Vec<McpIcon>,
    #[serde(default = "default_object")]
    pub annotations: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct McpIcon {
    pub src: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sizes: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RestOverlay {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default = "default_object")]
    pub path_params: Value,
    #[serde(default = "default_object")]
    pub query_params: Value,
    #[serde(default)]
    pub request_body_schema: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CliOverlay {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub about: Option<String>,
    #[serde(default)]
    pub long_about: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub flags: Vec<Value>,
    #[serde(default)]
    pub default_output: Option<String>,
    #[serde(default)]
    pub interactive: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PaletteOverlay {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub tone: Option<String>,
    #[serde(default)]
    pub arg_mode: Option<String>,
    #[serde(default)]
    pub result_view: Option<String>,
    #[serde(default)]
    pub aurora_blocks: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UiOverlay {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub aurora_registry_dependencies: Vec<String>,
    #[serde(default)]
    pub shadcn_items: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default = "default_object")]
    pub meta: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DocsOverlay {
    #[serde(default)]
    pub when_to_use: Option<String>,
    #[serde(default)]
    pub examples: Vec<ProviderExample>,
    #[serde(default)]
    pub troubleshooting: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PluginOverlay {
    #[serde(default = "default_true")]
    pub generate_skill: bool,
    #[serde(default = "default_true")]
    pub generate_claude: bool,
    #[serde(default = "default_true")]
    pub generate_codex: bool,
    #[serde(default = "default_true")]
    pub generate_gemini: bool,
    #[serde(default = "default_true")]
    pub generate_marketplace: bool,
    #[serde(default)]
    pub mcp_registration: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProviderLimits {
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub max_response_bytes: Option<usize>,
    #[serde(default)]
    pub max_input_bytes: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProviderExample {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub input: Option<Value>,
    #[serde(default)]
    pub output: Option<Value>,
    #[serde(default)]
    pub cli: Option<String>,
    #[serde(default)]
    pub rest: Option<Value>,
    #[serde(default)]
    pub mcp: Option<Value>,
}

fn default_true() -> bool {
    true
}

fn default_object() -> Value {
    Value::Object(serde_json::Map::new())
}
