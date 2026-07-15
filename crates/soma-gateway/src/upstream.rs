//! Self-contained upstream runtime primitives.

pub mod http_client;
pub mod pool;
pub mod relay;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    #[serde(default)]
    pub destructive: bool,
}

impl ToolDescriptor {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            input_schema: None,
            output_schema: None,
            destructive: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceDescriptor {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptDescriptor {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportKind {
    InProcess,
    HttpJson,
    HttpSse,
    Stdio,
    WebSocketUnsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpstreamHealth {
    Connected,
    Degraded {
        consecutive_failures: u32,
        error: Option<String>,
    },
    Disabled,
    Unsupported {
        reason: String,
    },
}

impl UpstreamHealth {
    #[must_use]
    pub fn is_routable(&self) -> bool {
        matches!(self, Self::Connected)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamSnapshot {
    pub name: String,
    pub transport: TransportKind,
    pub health: UpstreamHealth,
    pub tools: Vec<ToolDescriptor>,
    pub resources: Vec<ResourceDescriptor>,
    pub prompts: Vec<PromptDescriptor>,
    #[serde(default)]
    pub stale: bool,
}

impl UpstreamSnapshot {
    #[must_use]
    pub fn empty(name: impl Into<String>, transport: TransportKind) -> Self {
        Self {
            name: name.into(),
            transport,
            health: UpstreamHealth::Connected,
            tools: Vec::new(),
            resources: Vec::new(),
            prompts: Vec::new(),
            stale: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapScope {
    ToolsList,
    ToolsCall,
    ResourcesList,
    ResourcesRead,
    PromptsList,
    PromptsGet,
    RelayCall,
    HttpJson,
    HttpSseEvent,
    WebSocketFrame,
    StdioMessage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseCaps {
    pub tools_list_bytes: usize,
    pub tools_call_bytes: usize,
    pub resources_list_bytes: usize,
    pub resources_read_bytes: usize,
    pub prompts_list_bytes: usize,
    pub prompts_get_bytes: usize,
    pub relay_call_bytes: usize,
    pub http_json_bytes: usize,
    pub http_sse_event_bytes: usize,
    pub websocket_frame_bytes: usize,
    pub stdio_message_bytes: usize,
}

impl Default for ResponseCaps {
    fn default() -> Self {
        Self {
            tools_list_bytes: 512 * 1024,
            tools_call_bytes: 2 * 1024 * 1024,
            resources_list_bytes: 512 * 1024,
            resources_read_bytes: 2 * 1024 * 1024,
            prompts_list_bytes: 256 * 1024,
            prompts_get_bytes: 512 * 1024,
            relay_call_bytes: 4 * 1024 * 1024,
            http_json_bytes: 2 * 1024 * 1024,
            http_sse_event_bytes: 512 * 1024,
            websocket_frame_bytes: 512 * 1024,
            stdio_message_bytes: 2 * 1024 * 1024,
        }
    }
}

impl ResponseCaps {
    #[must_use]
    pub fn limit_for(&self, scope: CapScope) -> usize {
        match scope {
            CapScope::ToolsList => self.tools_list_bytes,
            CapScope::ToolsCall => self.tools_call_bytes,
            CapScope::ResourcesList => self.resources_list_bytes,
            CapScope::ResourcesRead => self.resources_read_bytes,
            CapScope::PromptsList => self.prompts_list_bytes,
            CapScope::PromptsGet => self.prompts_get_bytes,
            CapScope::RelayCall => self.relay_call_bytes,
            CapScope::HttpJson => self.http_json_bytes,
            CapScope::HttpSseEvent => self.http_sse_event_bytes,
            CapScope::WebSocketFrame => self.websocket_frame_bytes,
            CapScope::StdioMessage => self.stdio_message_bytes,
        }
    }

    pub fn enforce(&self, scope: CapScope, observed_bytes: usize) -> Result<(), UpstreamError> {
        let limit = self.limit_for(scope);
        if observed_bytes > limit {
            return Err(UpstreamError::ResponseTooLarge {
                scope,
                observed_bytes,
                limit,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum UpstreamError {
    #[error("upstream `{upstream}` is not configured")]
    UnknownUpstream { upstream: String },
    #[error("upstream `{upstream}` is not routable: {reason}")]
    NotRoutable { upstream: String, reason: String },
    #[error("upstream `{upstream}` does not expose `{item}`")]
    NotExposed { upstream: String, item: String },
    #[error("upstream `{upstream}` does not support `{capability}`")]
    Unsupported {
        upstream: String,
        capability: &'static str,
    },
    #[error("{scope:?} payload was {observed_bytes} bytes, exceeding {limit} bytes")]
    ResponseTooLarge {
        scope: CapScope,
        observed_bytes: usize,
        limit: usize,
    },
    #[error("tool params must be a JSON object")]
    ParamsMustBeObject,
}

#[cfg(test)]
#[path = "upstream_tests.rs"]
mod tests;
